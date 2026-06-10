use crate::bom::BomType;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AltMaterial {
    pub material_no: String,
    pub material_name: String,
    pub specification: String,
    pub supplier: String,
    pub unit_price: f64,
    pub delivery_days: u32,
    pub priority: u32,
    pub is_active: bool,
    pub stock_quantity: f64,
    pub unit: String,
}

impl AltMaterial {
    pub fn score(&self) -> f64 {
        let price_score = 1.0 / (self.unit_price + 1.0);
        let delivery_score = 1.0 / (self.delivery_days as f64 + 1.0);
        price_score * 0.6 + delivery_score * 0.4
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AltMaterialGroup {
    pub base_material_no: String,
    pub alternatives: Vec<AltMaterial>,
}

impl AltMaterialGroup {
    pub fn sorted_by_priority(&self) -> Vec<&AltMaterial> {
        let mut alts: Vec<&AltMaterial> = self
            .alternatives
            .iter()
            .filter(|a| a.is_active)
            .collect();
        alts.sort_by(|a, b| a.priority.cmp(&b.priority));
        alts
    }

    pub fn sorted_by_score(&self) -> Vec<&AltMaterial> {
        let mut alts: Vec<&AltMaterial> = self
            .alternatives
            .iter()
            .filter(|a| a.is_active)
            .collect();
        alts.sort_by(|a, b| {
            b.score()
                .partial_cmp(&a.score())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        alts
    }

    pub fn find_by_supplier(&self, supplier: &str) -> Option<&AltMaterial> {
        self.alternatives
            .iter()
            .find(|a| a.supplier == supplier && a.is_active)
    }
}

struct AltStateInner {
    groups: HashMap<String, AltMaterialGroup>,
    selected_index: HashMap<String, usize>,
}

pub struct AltMaterialManager {
    inner: Mutex<AltStateInner>,
    switch_count: AtomicUsize,
}

impl AltMaterialManager {
    pub fn new() -> Self {
        AltMaterialManager {
            inner: Mutex::new(AltStateInner {
                groups: HashMap::new(),
                selected_index: HashMap::new(),
            }),
            switch_count: AtomicUsize::new(0),
        }
    }

    pub fn add_group(&self, group: AltMaterialGroup) {
        let mut inner = self.inner.lock();
        let base_no = group.base_material_no.clone();
        inner.groups.insert(base_no.clone(), group);
        inner.selected_index.entry(base_no).or_insert(0);
    }

    pub fn get_group(&self, base_material_no: &str) -> Option<AltMaterialGroup> {
        let inner = self.inner.lock();
        inner.groups.get(base_material_no).cloned()
    }

    pub fn get_current_alt(&self, base_material_no: &str) -> Option<AltMaterial> {
        let inner = self.inner.lock();
        let group = inner.groups.get(base_material_no)?;
        let idx = inner.selected_index.get(base_material_no).copied().unwrap_or(0);
        let sorted = group.sorted_by_priority();
        sorted.get(idx).map(|a| (*a).clone())
    }

    pub fn try_switch_alt(
        &self,
        base_material_no: &str,
        required_qty: f64,
    ) -> Result<AltMaterial, String> {
        let mut inner = self.inner.lock();
        let group = inner
            .groups
            .get(base_material_no)
            .ok_or_else(|| format!("未找到物料 {} 的替代料组", base_material_no))?
            .clone();

        let current_idx = inner
            .selected_index
            .get(base_material_no)
            .copied()
            .unwrap_or(0);

        let sorted = group.sorted_by_priority();

        for (i, alt) in sorted.iter().enumerate() {
            if i <= current_idx {
                continue;
            }
            if alt.stock_quantity >= required_qty {
                inner
                    .selected_index
                    .insert(base_material_no.to_string(), i);
                self.switch_count.fetch_add(1, Ordering::SeqCst);
                return Ok((*alt).clone());
            }
        }

        Err(format!("物料 {} 没有足够库存的替代料", base_material_no))
    }

    pub fn reset_alt(&self, base_material_no: &str) -> Result<(), String> {
        let mut inner = self.inner.lock();
        if !inner.groups.contains_key(base_material_no) {
            return Err(format!("未找到物料 {} 的替代料组", base_material_no));
        }
        inner.selected_index.insert(base_material_no.to_string(), 0);
        Ok(())
    }

    pub fn update_stock(
        &self,
        base_material_no: &str,
        supplier: &str,
        new_stock: f64,
    ) -> Result<(), String> {
        let mut inner = self.inner.lock();
        let group = inner
            .groups
            .get_mut(base_material_no)
            .ok_or_else(|| format!("未找到物料 {} 的替代料组", base_material_no))?;

        let alt = group
            .alternatives
            .iter_mut()
            .find(|a| a.supplier == supplier)
            .ok_or_else(|| format!("未找到供应商 {} 的替代料", supplier))?;

        alt.stock_quantity = new_stock;
        Ok(())
    }

    pub fn consume_stock(
        &self,
        base_material_no: &str,
        supplier: &str,
        qty: f64,
    ) -> Result<f64, String> {
        let mut inner = self.inner.lock();
        let group = inner
            .groups
            .get_mut(base_material_no)
            .ok_or_else(|| format!("未找到物料 {} 的替代料组", base_material_no))?;

        let alt = group
            .alternatives
            .iter_mut()
            .find(|a| a.supplier == supplier)
            .ok_or_else(|| format!("未找到供应商 {} 的替代料", supplier))?;

        if alt.stock_quantity < qty {
            return Err(format!(
                "供应商 {} 库存不足，当前: {}, 需要: {}",
                supplier, alt.stock_quantity, qty
            ));
        }

        alt.stock_quantity -= qty;
        Ok(alt.stock_quantity)
    }

    pub fn switch_count(&self) -> usize {
        self.switch_count.load(Ordering::SeqCst)
    }

    pub fn all_groups(&self) -> Vec<AltMaterialGroup> {
        let inner = self.inner.lock();
        inner.groups.values().cloned().collect()
    }
}

impl Default for AltMaterialManager {
    fn default() -> Self {
        Self::new()
    }
}

pub fn is_virtual_material(bom_type: &BomType) -> bool {
    matches!(bom_type, BomType::Virtual)
}

pub fn should_skip_in_cost(bom_type: &BomType) -> bool {
    matches!(bom_type, BomType::Virtual)
}

pub fn collect_non_virtual_alts(group: &AltMaterialGroup) -> Vec<&AltMaterial> {
    group
        .alternatives
        .iter()
        .filter(|a| a.is_active)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_group() -> AltMaterialGroup {
        AltMaterialGroup {
            base_material_no: "M001".to_string(),
            alternatives: vec![
                AltMaterial {
                    material_no: "M001-A".to_string(),
                    material_name: "螺栓A".to_string(),
                    specification: "M10".to_string(),
                    supplier: "供应商A".to_string(),
                    unit_price: 1.5,
                    delivery_days: 3,
                    priority: 1,
                    is_active: true,
                    stock_quantity: 100.0,
                    unit: "个".to_string(),
                },
                AltMaterial {
                    material_no: "M001-B".to_string(),
                    material_name: "螺栓B".to_string(),
                    specification: "M10".to_string(),
                    supplier: "供应商B".to_string(),
                    unit_price: 2.0,
                    delivery_days: 1,
                    priority: 2,
                    is_active: true,
                    stock_quantity: 50.0,
                    unit: "个".to_string(),
                },
                AltMaterial {
                    material_no: "M001-C".to_string(),
                    material_name: "螺栓C".to_string(),
                    specification: "M10".to_string(),
                    supplier: "供应商C".to_string(),
                    unit_price: 1.8,
                    delivery_days: 5,
                    priority: 3,
                    is_active: true,
                    stock_quantity: 200.0,
                    unit: "个".to_string(),
                },
            ],
        }
    }

    #[test]
    fn test_alt_sorting() {
        let group = sample_group();
        let sorted = group.sorted_by_priority();
        assert_eq!(sorted.len(), 3);
        assert_eq!(sorted[0].supplier, "供应商A");
        assert_eq!(sorted[1].supplier, "供应商B");
        assert_eq!(sorted[2].supplier, "供应商C");
    }

    #[test]
    fn test_try_switch_alt() {
        let manager = AltMaterialManager::new();
        manager.add_group(sample_group());

        let current = manager.get_current_alt("M001").unwrap();
        assert_eq!(current.supplier, "供应商A");

        let result = manager.try_switch_alt("M001", 60.0);
        assert!(result.is_ok());
        let new_alt = result.unwrap();
        assert_eq!(new_alt.supplier, "供应商C");

        let current2 = manager.get_current_alt("M001").unwrap();
        assert_eq!(current2.supplier, "供应商C");
    }

    #[test]
    fn test_consume_stock() {
        let manager = AltMaterialManager::new();
        manager.add_group(sample_group());

        let remaining = manager.consume_stock("M001", "供应商A", 30.0).unwrap();
        assert_eq!(remaining, 70.0);

        let result = manager.consume_stock("M001", "供应商A", 80.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_concurrent_switch() {
        use std::sync::Arc;
        use std::thread;

        let manager = Arc::new(AltMaterialManager::new());
        manager.add_group(sample_group());

        let mut handles = vec![];

        for _ in 0..10 {
            let mgr = manager.clone();
            handles.push(thread::spawn(move || {
                let _ = mgr.try_switch_alt("M001", 150.0);
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let current = manager.get_current_alt("M001").unwrap();
        assert_eq!(current.supplier, "供应商C");
    }

    #[test]
    fn test_virtual_check() {
        assert!(is_virtual_material(&BomType::Virtual));
        assert!(!is_virtual_material(&BomType::RawMaterial));
        assert!(should_skip_in_cost(&BomType::Virtual));
    }
}
