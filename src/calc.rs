use crate::bom::BomType;
use crate::tree::{self, BomTreeNode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceTier {
    pub min_qty: f64,
    pub max_qty: f64,
    pub unit_price: f64,
}

impl PriceTier {
    pub fn matches(&self, qty: f64) -> bool {
        qty >= self.min_qty && qty <= self.max_qty
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceTiersConfig {
    pub tiers: Vec<PriceTier>,
}

impl Default for PriceTiersConfig {
    fn default() -> Self {
        PriceTiersConfig {
            tiers: vec![
                PriceTier {
                    min_qty: 1.0,
                    max_qty: 100.0,
                    unit_price: 1.0,
                },
                PriceTier {
                    min_qty: 101.0,
                    max_qty: 500.0,
                    unit_price: 0.9,
                },
                PriceTier {
                    min_qty: 501.0,
                    max_qty: 2000.0,
                    unit_price: 0.8,
                },
                PriceTier {
                    min_qty: 2001.0,
                    max_qty: f64::MAX,
                    unit_price: 0.7,
                },
            ],
        }
    }
}

impl PriceTiersConfig {
    pub fn get_price_multiplier(&self, qty: f64) -> f64 {
        for tier in &self.tiers {
            if tier.matches(qty) {
                return tier.unit_price;
            }
        }
        self.tiers.last().map(|t| t.unit_price).unwrap_or(1.0)
    }

    pub fn get_tier_name(&self, qty: f64) -> String {
        for (i, tier) in self.tiers.iter().enumerate() {
            if tier.matches(qty) {
                return format!("第{}档 ({}~{})", i + 1, tier.min_qty, tier.max_qty);
            }
        }
        match self.tiers.last() {
            Some(tier) => format!(
                "第{}档 ({}~{})",
                self.tiers.len(),
                tier.min_qty,
                tier.max_qty
            ),
            None => "默认档位".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurchargeConfig {
    pub processing_fee_rate: f64,
    pub packaging_fee_rate: f64,
    pub management_fee_rate: f64,
}

impl Default for SurchargeConfig {
    fn default() -> Self {
        SurchargeConfig {
            processing_fee_rate: 0.15,
            packaging_fee_rate: 0.05,
            management_fee_rate: 0.08,
        }
    }
}

impl SurchargeConfig {
    pub fn total_rate(&self) -> f64 {
        self.processing_fee_rate + self.packaging_fee_rate + self.management_fee_rate
    }

    pub fn apply(&self, base_cost: f64) -> SurchargeDetail {
        let processing = base_cost * self.processing_fee_rate;
        let packaging = base_cost * self.packaging_fee_rate;
        let management = base_cost * self.management_fee_rate;
        SurchargeDetail {
            processing_fee: processing,
            packaging_fee: packaging,
            management_fee: management,
            total_surcharge: processing + packaging + management,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SurchargeDetail {
    pub processing_fee: f64,
    pub packaging_fee: f64,
    pub management_fee: f64,
    pub total_surcharge: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostBreakdown {
    pub material_no: String,
    pub material_name: String,
    pub quantity: f64,
    pub unit: String,
    pub unit_cost: f64,
    pub total_cost: f64,
    pub loss_quantity: f64,
    pub loss_cost: f64,
    pub raw_material_cost: f64,
    pub surcharge: Option<SurchargeDetail>,
    pub children: Vec<CostBreakdown>,
    pub is_virtual: bool,
    pub price_tier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostCalculationResult {
    pub total_cost: f64,
    pub total_quantity: f64,
    pub breakdown: Vec<CostBreakdown>,
    pub price_tiers_config: PriceTiersConfig,
    pub surcharge_config: SurchargeConfig,
}

pub struct CostCalculator {
    price_tiers: PriceTiersConfig,
    surcharge: SurchargeConfig,
    custom_prices: HashMap<String, Vec<PriceTier>>,
}

impl CostCalculator {
    pub fn new() -> Self {
        CostCalculator {
            price_tiers: PriceTiersConfig::default(),
            surcharge: SurchargeConfig::default(),
            custom_prices: HashMap::new(),
        }
    }

    pub fn price_tiers(&self) -> &PriceTiersConfig {
        &self.price_tiers
    }

    pub fn surcharge_config(&self) -> &SurchargeConfig {
        &self.surcharge
    }

    pub fn with_price_tiers(mut self, tiers: PriceTiersConfig) -> Self {
        self.price_tiers = tiers;
        self
    }

    pub fn with_surcharge(mut self, surcharge: SurchargeConfig) -> Self {
        self.surcharge = surcharge;
        self
    }

    pub fn set_custom_price_tiers(&mut self, material_no: &str, tiers: Vec<PriceTier>) {
        self.custom_prices.insert(material_no.to_string(), tiers);
    }

    pub fn calculate(
        &self,
        tree: &[BomTreeNode],
        order_quantity: f64,
    ) -> CostCalculationResult {
        let mut breakdown = Vec::new();
        let mut total_cost = 0.0;

        for root in tree {
            let bd = self.calculate_node(root, order_quantity, 1);
            total_cost += bd.total_cost;
            breakdown.push(bd);
        }

        CostCalculationResult {
            total_cost,
            total_quantity: order_quantity,
            breakdown,
            price_tiers_config: self.price_tiers.clone(),
            surcharge_config: self.surcharge.clone(),
        }
    }

    fn calculate_node(
        &self,
        node: &BomTreeNode,
        order_quantity: f64,
        depth: usize,
    ) -> CostBreakdown {
        if node.is_virtual() {
            let mut children = Vec::new();
            let mut total_cost = 0.0;
            for child in &node.children {
                let child_bd = self.calculate_node(child, order_quantity, depth + 1);
                total_cost += child_bd.total_cost;
                children.push(child_bd);
            }
            return CostBreakdown {
                material_no: node.material_no.clone(),
                material_name: node.material_name.clone(),
                quantity: node.quantity * order_quantity,
                unit: node.unit.clone(),
                unit_cost: 0.0,
                total_cost,
                loss_quantity: 0.0,
                loss_cost: 0.0,
                raw_material_cost: total_cost,
                surcharge: None,
                children,
                is_virtual: true,
                price_tier: "虚拟件".to_string(),
            };
        }

        let node_qty = node.quantity * order_quantity;
        let loss_qty = node_qty * node.loss_rate;
        let total_qty_with_loss = node_qty + loss_qty;

        let price_multiplier = self.get_price_multiplier_for(&node.material_no, total_qty_with_loss);
        let tier_name = self.get_tier_name_for(&node.material_no, total_qty_with_loss);

        let base_unit_price = node.unit_price * price_multiplier;
        let unit_cost = base_unit_price;

        let mut children_cost = 0.0;
        let mut children = Vec::new();

        for child in &node.children {
            let child_qty = child.quantity * total_qty_with_loss;
            let child_bd = self.calculate_node(child, child_qty, depth + 1);
            children_cost += child_bd.total_cost;
            children.push(child_bd);
        }

        let raw_material_cost = children_cost;
        let total_base_cost = raw_material_cost + unit_cost * node_qty;

        let (surcharge, total_cost) = if matches!(node.bom_type, BomType::SemiFinished)
            || matches!(node.bom_type, BomType::FinishedProduct)
        {
            let sd = self.surcharge.apply(total_base_cost);
            let tc = total_base_cost + sd.total_surcharge;
            (Some(sd), tc)
        } else {
            (None, total_base_cost)
        };

        let loss_cost = loss_qty * unit_cost;

        CostBreakdown {
            material_no: node.material_no.clone(),
            material_name: node.material_name.clone(),
            quantity: node_qty,
            unit: node.unit.clone(),
            unit_cost,
            total_cost,
            loss_quantity: loss_qty,
            loss_cost,
            raw_material_cost,
            surcharge,
            children,
            is_virtual: false,
            price_tier: tier_name,
        }
    }

    fn get_price_multiplier_for(&self, material_no: &str, qty: f64) -> f64 {
        if let Some(tiers) = self.custom_prices.get(material_no) {
            for tier in tiers {
                if tier.matches(qty) {
                    return tier.unit_price;
                }
            }
            return tiers.last().map(|t| t.unit_price).unwrap_or(1.0);
        }
        self.price_tiers.get_price_multiplier(qty)
    }

    fn get_tier_name_for(&self, material_no: &str, qty: f64) -> String {
        if let Some(tiers) = self.custom_prices.get(material_no) {
            for (i, tier) in tiers.iter().enumerate() {
                if tier.matches(qty) {
                    return format!("第{}档 ({}~{})", i + 1, tier.min_qty, tier.max_qty);
                }
            }
            return "自定义末档".to_string();
        }
        self.price_tiers.get_tier_name(qty)
    }
}

impl Default for CostCalculator {
    fn default() -> Self {
        Self::new()
    }
}

pub fn format_cost(value: f64) -> String {
    format!("{:.2}", value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bom::{BomType, SourceType};
    use crate::tree::BomTreeNode;

    fn sample_tree() -> Vec<BomTreeNode> {
        let leaf1 = BomTreeNode {
            material_no: "R001".to_string(),
            material_name: "原材料1".to_string(),
            specification: "".to_string(),
            bom_type: BomType::RawMaterial,
            quantity: 2.0,
            unit: "kg".to_string(),
            loss_rate: 0.05,
            supplier: "供应商A".to_string(),
            unit_price: 10.0,
            source: SourceType::Purchased,
            effective_date: "".to_string(),
            expiry_date: "".to_string(),
            remark: "".to_string(),
            children: vec![],
            level: 1,
            path: "A001/R001".to_string(),
        };

        let semi = BomTreeNode {
            material_no: "S001".to_string(),
            material_name: "半成品1".to_string(),
            specification: "".to_string(),
            bom_type: BomType::SemiFinished,
            quantity: 1.0,
            unit: "件".to_string(),
            loss_rate: 0.03,
            supplier: "".to_string(),
            unit_price: 5.0,
            source: SourceType::SelfMade,
            effective_date: "".to_string(),
            expiry_date: "".to_string(),
            remark: "".to_string(),
            children: vec![leaf1],
            level: 0,
            path: "A001/S001".to_string(),
        };

        let root = BomTreeNode {
            material_no: "A001".to_string(),
            material_name: "成品A".to_string(),
            specification: "".to_string(),
            bom_type: BomType::FinishedProduct,
            quantity: 1.0,
            unit: "件".to_string(),
            loss_rate: 0.02,
            supplier: "".to_string(),
            unit_price: 20.0,
            source: SourceType::SelfMade,
            effective_date: "".to_string(),
            expiry_date: "".to_string(),
            remark: "".to_string(),
            children: vec![semi],
            level: 0,
            path: "A001".to_string(),
        };

        vec![root]
    }

    #[test]
    fn test_price_tiers_default() {
        let tiers = PriceTiersConfig::default();
        assert_eq!(tiers.get_price_multiplier(50.0), 1.0);
        assert_eq!(tiers.get_price_multiplier(200.0), 0.9);
        assert_eq!(tiers.get_price_multiplier(1000.0), 0.8);
        assert_eq!(tiers.get_price_multiplier(5000.0), 0.7);
    }

    #[test]
    fn test_surcharge_calc() {
        let sur = SurchargeConfig::default();
        let detail = sur.apply(100.0);
        assert_eq!(detail.processing_fee, 15.0);
        assert_eq!(detail.packaging_fee, 5.0);
        assert_eq!(detail.management_fee, 8.0);
        assert_eq!(detail.total_surcharge, 28.0);
    }

    #[test]
    fn test_cost_calculation_basic() {
        let tree = sample_tree();
        let calc = CostCalculator::new();
        let result = calc.calculate(&tree, 100.0);

        assert!(result.total_cost > 0.0);
        assert_eq!(result.total_quantity, 100.0);
        assert_eq!(result.breakdown.len(), 1);
    }

    #[test]
    fn test_virtual_node() {
        let mut tree = sample_tree();
        let virtual_node = BomTreeNode {
            material_no: "V001".to_string(),
            material_name: "虚拟工序".to_string(),
            specification: "".to_string(),
            bom_type: BomType::Virtual,
            quantity: 1.0,
            unit: "工序".to_string(),
            loss_rate: 0.0,
            supplier: "".to_string(),
            unit_price: 0.0,
            source: SourceType::SelfMade,
            effective_date: "".to_string(),
            expiry_date: "".to_string(),
            remark: "".to_string(),
            children: vec![],
            level: 1,
            path: "A001/V001".to_string(),
        };
        tree[0].children.push(virtual_node);

        let calc = CostCalculator::new();
        let result = calc.calculate(&tree, 100.0);
        assert!(result.total_cost > 0.0);
    }

    #[test]
    fn test_custom_price_tiers() {
        let mut calc = CostCalculator::new();
        calc.set_custom_price_tiers(
            "R001",
            vec![
                PriceTier {
                    min_qty: 1.0,
                    max_qty: 50.0,
                    unit_price: 1.0,
                },
                PriceTier {
                    min_qty: 51.0,
                    max_qty: f64::MAX,
                    unit_price: 0.5,
                },
            ],
        );

        let tree = sample_tree();
        let result = calc.calculate(&tree, 10.0);
        assert!(result.total_cost > 0.0);
    }

    #[test]
    fn test_tier_name_consistent_with_multiplier() {
        let tiers = PriceTiersConfig::default();

        let last_tier = tiers.tiers.last().unwrap();
        let last_multiplier = last_tier.unit_price;
        let last_tier_index = tiers.tiers.len();

        let fallback_multiplier = tiers.get_price_multiplier(f64::MAX);
        let fallback_name = tiers.get_tier_name(f64::MAX);

        assert_eq!(fallback_multiplier, last_multiplier);
        assert!(
            fallback_name.contains(&format!("第{}档", last_tier_index)),
            "fallback 档位名应该与最后一档一致，实际为: {}",
            fallback_name
        );
        assert!(
            !fallback_name.contains("未知"),
            "fallback 档位名不应为'未知档位'，实际为: {}",
            fallback_name
        );

        for (i, tier) in tiers.tiers.iter().enumerate() {
            let mid_qty = (tier.min_qty + tier.max_qty) / 2.0;
            let name = tiers.get_tier_name(mid_qty);
            assert!(
                name.contains(&format!("第{}档", i + 1)),
                "数量 {} 应该匹配第{}档，实际档位名: {}",
                mid_qty,
                i + 1,
                name
            );
        }
    }

    #[test]
    fn test_empty_tiers_fallback() {
        let empty_tiers = PriceTiersConfig { tiers: vec![] };
        assert_eq!(empty_tiers.get_price_multiplier(100.0), 1.0);
        assert_eq!(empty_tiers.get_tier_name(100.0), "默认档位");
    }
}
