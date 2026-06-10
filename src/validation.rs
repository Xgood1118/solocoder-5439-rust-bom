use crate::bom::BomData;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationIssue {
    pub level: IssueLevel,
    pub code: String,
    pub message: String,
    pub material_no: Option<String>,
    pub field: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueLevel {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub issues: Vec<ValidationIssue>,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
}

impl ValidationResult {
    pub fn new() -> Self {
        ValidationResult {
            valid: true,
            issues: Vec::new(),
            error_count: 0,
            warning_count: 0,
            info_count: 0,
        }
    }

    pub fn add_issue(&mut self, issue: ValidationIssue) {
        match issue.level {
            IssueLevel::Error => {
                self.valid = false;
                self.error_count += 1;
            }
            IssueLevel::Warning => self.warning_count += 1,
            IssueLevel::Info => self.info_count += 1,
        }
        self.issues.push(issue);
    }

    pub fn merge(&mut self, other: ValidationResult) {
        for issue in other.issues {
            self.add_issue(issue);
        }
    }
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self::new()
    }
}

pub struct BomValidator {
    master_data: HashSet<String>,
    max_loss_rate: f64,
}

impl BomValidator {
    pub fn new() -> Self {
        BomValidator {
            master_data: HashSet::new(),
            max_loss_rate: 0.5,
        }
    }

    pub fn with_max_loss_rate(mut self, max_loss_rate: f64) -> Self {
        self.max_loss_rate = max_loss_rate;
        self
    }

    pub fn load_master_data(&mut self, material_nos: Vec<String>) {
        self.master_data = material_nos.into_iter().collect();
    }

    pub fn add_master_material(&mut self, material_no: &str) {
        self.master_data.insert(material_no.to_string());
    }

    pub fn validate(&self, items: &BomData) -> ValidationResult {
        let mut result = ValidationResult::new();

        result.merge(self.validate_fields(items));
        result.merge(self.validate_master_data(items));
        result.merge(self.validate_quantities(items));
        result.merge(self.validate_loss_rates(items));
        result.merge(self.validate_circular_reference(items));

        result
    }

    fn validate_fields(&self, items: &BomData) -> ValidationResult {
        let mut result = ValidationResult::new();
        let mut seen = HashSet::new();

        for item in items {
            if item.material_no.is_empty() {
                result.add_issue(ValidationIssue {
                    level: IssueLevel::Error,
                    code: "EMPTY_MATERIAL_NO".to_string(),
                    message: "物料号不能为空".to_string(),
                    material_no: None,
                    field: Some("物料号".to_string()),
                });
                continue;
            }

            if !seen.insert(item.material_no.clone()) {
                result.add_issue(ValidationIssue {
                    level: IssueLevel::Error,
                    code: "DUPLICATE_MATERIAL_NO".to_string(),
                    message: format!("物料号 {} 重复", item.material_no),
                    material_no: Some(item.material_no.clone()),
                    field: Some("物料号".to_string()),
                });
            }

            if item.material_name.is_empty() {
                result.add_issue(ValidationIssue {
                    level: IssueLevel::Warning,
                    code: "EMPTY_MATERIAL_NAME".to_string(),
                    message: format!("物料 {} 名称为空", item.material_no),
                    material_no: Some(item.material_no.clone()),
                    field: Some("物料名称".to_string()),
                });
            }

            if item.unit.is_empty() {
                result.add_issue(ValidationIssue {
                    level: IssueLevel::Warning,
                    code: "EMPTY_UNIT".to_string(),
                    message: format!("物料 {} 单位为空", item.material_no),
                    material_no: Some(item.material_no.clone()),
                    field: Some("单位".to_string()),
                });
            }
        }

        result
    }

    fn validate_master_data(&self, items: &BomData) -> ValidationResult {
        let mut result = ValidationResult::new();

        if self.master_data.is_empty() {
            return result;
        }

        for item in items {
            if !self.master_data.contains(&item.material_no) {
                result.add_issue(ValidationIssue {
                    level: IssueLevel::Error,
                    code: "MATERIAL_NOT_IN_MASTER".to_string(),
                    message: format!("物料号 {} 不在主数据中", item.material_no),
                    material_no: Some(item.material_no.clone()),
                    field: Some("物料号".to_string()),
                });
            }
        }

        result
    }

    fn validate_quantities(&self, items: &BomData) -> ValidationResult {
        let mut result = ValidationResult::new();

        for item in items {
            if item.quantity <= 0.0 {
                result.add_issue(ValidationIssue {
                    level: IssueLevel::Error,
                    code: "INVALID_QUANTITY".to_string(),
                    message: format!(
                        "物料 {} 数量 {} 必须为正数",
                        item.material_no, item.quantity
                    ),
                    material_no: Some(item.material_no.clone()),
                    field: Some("数量".to_string()),
                });
            }

            if item.unit_price < 0.0 {
                result.add_issue(ValidationIssue {
                    level: IssueLevel::Error,
                    code: "NEGATIVE_PRICE".to_string(),
                    message: format!(
                        "物料 {} 单价 {} 不能为负数",
                        item.material_no, item.unit_price
                    ),
                    material_no: Some(item.material_no.clone()),
                    field: Some("单价".to_string()),
                });
            }
        }

        result
    }

    fn validate_loss_rates(&self, items: &BomData) -> ValidationResult {
        let mut result = ValidationResult::new();

        for item in items {
            if item.loss_rate < 0.0 {
                result.add_issue(ValidationIssue {
                    level: IssueLevel::Error,
                    code: "NEGATIVE_LOSS_RATE".to_string(),
                    message: format!(
                        "物料 {} 损耗率 {} 不能为负数",
                        item.material_no, item.loss_rate
                    ),
                    material_no: Some(item.material_no.clone()),
                    field: Some("损耗率".to_string()),
                });
            }

            if item.loss_rate > self.max_loss_rate {
                result.add_issue(ValidationIssue {
                    level: IssueLevel::Error,
                    code: "LOSS_RATE_TOO_HIGH".to_string(),
                    message: format!(
                        "物料 {} 损耗率 {:.1}% 超过上限 {:.1}%",
                        item.material_no,
                        item.loss_rate * 100.0,
                        self.max_loss_rate * 100.0
                    ),
                    material_no: Some(item.material_no.clone()),
                    field: Some("损耗率".to_string()),
                });
            }
        }

        result
    }

    fn validate_circular_reference(&self, items: &BomData) -> ValidationResult {
        let mut result = ValidationResult::new();

        let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut all_materials = HashSet::new();

        for item in items {
            all_materials.insert(item.material_no.clone());
            if let Some(parent) = &item.parent_material_no {
                if !parent.is_empty() {
                    children_map
                        .entry(parent.clone())
                        .or_insert_with(Vec::new)
                        .push(item.material_no.clone());
                }
            }
        }

        let mut visited = HashSet::new();
        let mut path = Vec::new();
        let mut found_cycles = HashSet::new();

        for material in &all_materials {
            self.detect_cycle_dfs(
                material,
                &children_map,
                &mut visited,
                &mut path,
                &mut result,
                &mut found_cycles,
            );
        }

        result
    }

    fn detect_cycle_dfs(
        &self,
        material: &str,
        children_map: &HashMap<String, Vec<String>>,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
        result: &mut ValidationResult,
        found_cycles: &mut HashSet<String>,
    ) {
        if path.iter().any(|m| m == material) {
            let cycle_key = path.iter()
                .skip_while(|m| *m != material)
                .cloned()
                .collect::<Vec<_>>()
                .join("->");

            if found_cycles.insert(cycle_key.clone()) {
                let cycle_start = path.iter().position(|m| m == material).unwrap();
                let cycle: Vec<&str> = path[cycle_start..]
                    .iter()
                    .map(|s| s.as_str())
                    .collect();
                let cycle_str = format!("{} -> {}", cycle.join(" -> "), material);
                result.add_issue(ValidationIssue {
                    level: IssueLevel::Error,
                    code: "CIRCULAR_REFERENCE".to_string(),
                    message: format!("检测到循环引用: {}", cycle_str),
                    material_no: Some(material.to_string()),
                    field: Some("父件关系".to_string()),
                });
            }
            return;
        }

        if visited.contains(material) {
            return;
        }

        visited.insert(material.to_string());
        path.push(material.to_string());

        if let Some(children) = children_map.get(material) {
            for child in children {
                self.detect_cycle_dfs(child, children_map, visited, path, result, found_cycles);
            }
        }

        path.pop();
    }
}

impl Default for BomValidator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bom::{BomItem, BomType, SourceType};

    fn sample_bom_data() -> BomData {
        vec![
            BomItem {
                material_no: "A001".to_string(),
                material_name: "成品A".to_string(),
                specification: "".to_string(),
                bom_type: BomType::FinishedProduct,
                quantity: 1.0,
                unit: "件".to_string(),
                loss_rate: 0.05,
                supplier: "".to_string(),
                unit_price: 100.0,
                source: SourceType::SelfMade,
                effective_date: "".to_string(),
                expiry_date: "".to_string(),
                remark: "".to_string(),
                parent_material_no: None,
            },
            BomItem {
                material_no: "B001".to_string(),
                material_name: "半成品B".to_string(),
                specification: "".to_string(),
                bom_type: BomType::SemiFinished,
                quantity: 2.0,
                unit: "件".to_string(),
                loss_rate: 0.03,
                supplier: "".to_string(),
                unit_price: 50.0,
                source: SourceType::SelfMade,
                effective_date: "".to_string(),
                expiry_date: "".to_string(),
                remark: "".to_string(),
                parent_material_no: Some("A001".to_string()),
            },
        ]
    }

    #[test]
    fn test_valid_bom() {
        let validator = BomValidator::new();
        let data = sample_bom_data();
        let result = validator.validate(&data);
        assert!(result.valid);
    }

    #[test]
    fn test_invalid_quantity() {
        let validator = BomValidator::new();
        let mut data = sample_bom_data();
        data[0].quantity = -1.0;

        let result = validator.validate(&data);
        assert!(!result.valid);
        assert!(result.issues.iter().any(|i| i.code == "INVALID_QUANTITY"));
    }

    #[test]
    fn test_loss_rate_too_high() {
        let validator = BomValidator::new();
        let mut data = sample_bom_data();
        data[0].loss_rate = 1.5;

        let result = validator.validate(&data);
        assert!(!result.valid);
        assert!(result.issues.iter().any(|i| i.code == "LOSS_RATE_TOO_HIGH"));
    }

    #[test]
    fn test_circular_reference() {
        let validator = BomValidator::new();
        let data = vec![
            BomItem {
                material_no: "A001".to_string(),
                material_name: "物料A".to_string(),
                specification: "".to_string(),
                bom_type: BomType::SemiFinished,
                quantity: 1.0,
                unit: "件".to_string(),
                loss_rate: 0.01,
                supplier: "".to_string(),
                unit_price: 10.0,
                source: SourceType::SelfMade,
                effective_date: "".to_string(),
                expiry_date: "".to_string(),
                remark: "".to_string(),
                parent_material_no: Some("B001".to_string()),
            },
            BomItem {
                material_no: "B001".to_string(),
                material_name: "物料B".to_string(),
                specification: "".to_string(),
                bom_type: BomType::SemiFinished,
                quantity: 1.0,
                unit: "件".to_string(),
                loss_rate: 0.01,
                supplier: "".to_string(),
                unit_price: 20.0,
                source: SourceType::SelfMade,
                effective_date: "".to_string(),
                expiry_date: "".to_string(),
                remark: "".to_string(),
                parent_material_no: Some("A001".to_string()),
            },
        ];

        let result = validator.validate(&data);
        assert!(!result.valid);
        assert!(result
            .issues
            .iter()
            .any(|i| i.code == "CIRCULAR_REFERENCE"));
    }

    #[test]
    fn test_master_data_validation() {
        let mut validator = BomValidator::new();
        validator.add_master_material("A001");

        let data = sample_bom_data();
        let result = validator.validate(&data);
        assert!(!result.valid);
        assert!(result
            .issues
            .iter()
            .any(|i| i.code == "MATERIAL_NOT_IN_MASTER"));
    }

    #[test]
    fn test_duplicate_material() {
        let validator = BomValidator::new();
        let data = vec![
            BomItem {
                material_no: "A001".to_string(),
                material_name: "物料A".to_string(),
                specification: "".to_string(),
                bom_type: BomType::RawMaterial,
                quantity: 1.0,
                unit: "个".to_string(),
                loss_rate: 0.01,
                supplier: "".to_string(),
                unit_price: 10.0,
                source: SourceType::Purchased,
                effective_date: "".to_string(),
                expiry_date: "".to_string(),
                remark: "".to_string(),
                parent_material_no: None,
            },
            BomItem {
                material_no: "A001".to_string(),
                material_name: "物料A-重复".to_string(),
                specification: "".to_string(),
                bom_type: BomType::RawMaterial,
                quantity: 2.0,
                unit: "个".to_string(),
                loss_rate: 0.02,
                supplier: "".to_string(),
                unit_price: 12.0,
                source: SourceType::Purchased,
                effective_date: "".to_string(),
                expiry_date: "".to_string(),
                remark: "".to_string(),
                parent_material_no: None,
            },
        ];

        let result = validator.validate(&data);
        assert!(!result.valid);
        assert!(result
            .issues
            .iter()
            .any(|i| i.code == "DUPLICATE_MATERIAL_NO"));
    }
}
