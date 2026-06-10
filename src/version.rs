use crate::bom::BomData;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BomVersion {
    pub version_id: String,
    pub version_number: u32,
    pub bom_id: String,
    pub data: BomData,
    pub created_at: DateTime<Utc>,
    pub modified_by: String,
    pub change_reason: String,
    pub change_description: Vec<ChangeItem>,
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeItem {
    pub field: String,
    pub old_value: String,
    pub new_value: String,
    pub material_no: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionHistory {
    pub bom_id: String,
    pub versions: Vec<BomVersion>,
    pub current_version: u32,
}

impl VersionHistory {
    pub fn new(bom_id: &str, initial_data: BomData, created_by: &str, reason: &str) -> Self {
        let first_version = BomVersion {
            version_id: Uuid::new_v4().to_string(),
            version_number: 1,
            bom_id: bom_id.to_string(),
            data: initial_data,
            created_at: Utc::now(),
            modified_by: created_by.to_string(),
            change_reason: reason.to_string(),
            change_description: vec![ChangeItem {
                field: "BOM".to_string(),
                old_value: String::new(),
                new_value: "初始版本".to_string(),
                material_no: None,
            }],
            is_current: true,
        };

        VersionHistory {
            bom_id: bom_id.to_string(),
            versions: vec![first_version],
            current_version: 1,
        }
    }

    pub fn add_version(
        &mut self,
        new_data: BomData,
        modified_by: &str,
        reason: &str,
    ) -> &BomVersion {
        let old_version = self.current();
        let changes = diff_bom(&old_version.data, &new_data);

        let new_version_number = self.current_version + 1;
        let new_version = BomVersion {
            version_id: Uuid::new_v4().to_string(),
            version_number: new_version_number,
            bom_id: self.bom_id.clone(),
            data: new_data,
            created_at: Utc::now(),
            modified_by: modified_by.to_string(),
            change_reason: reason.to_string(),
            change_description: changes,
            is_current: true,
        };

        for v in &mut self.versions {
            v.is_current = false;
        }

        self.versions.push(new_version);
        self.current_version = new_version_number;

        self.current()
    }

    pub fn current(&self) -> &BomVersion {
        self.versions
            .iter()
            .find(|v| v.is_current)
            .unwrap_or_else(|| self.versions.last().expect("至少有一个版本"))
    }

    pub fn rollback(&mut self, version_number: u32, modified_by: &str, reason: &str) -> Result<&BomVersion, String> {
        let target_version = self
            .versions
            .iter()
            .find(|v| v.version_number == version_number)
            .ok_or_else(|| format!("版本 {} 不存在", version_number))?;

        let rolled_back_data = target_version.data.clone();
        let rollback_reason = format!("回滚到版本 {}: {}", version_number, reason);

        Ok(self.add_version(rolled_back_data, modified_by, &rollback_reason))
    }

    pub fn get_version(&self, version_number: u32) -> Option<&BomVersion> {
        self.versions.iter().find(|v| v.version_number == version_number)
    }

    pub fn version_count(&self) -> usize {
        self.versions.len()
    }

    pub fn list_versions(&self) -> Vec<BomVersionSummary> {
        self.versions
            .iter()
            .map(|v| BomVersionSummary {
                version_number: v.version_number,
                version_id: v.version_id.clone(),
                created_at: v.created_at,
                modified_by: v.modified_by.clone(),
                change_reason: v.change_reason.clone(),
                change_count: v.change_description.len(),
                is_current: v.is_current,
            })
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BomVersionSummary {
    pub version_number: u32,
    pub version_id: String,
    pub created_at: DateTime<Utc>,
    pub modified_by: String,
    pub change_reason: String,
    pub change_count: usize,
    pub is_current: bool,
}

pub fn diff_bom(old: &BomData, new: &BomData) -> Vec<ChangeItem> {
    let mut changes = Vec::new();

    let old_map: HashMap<&str, _> = old
        .iter()
        .map(|item| (item.material_no.as_str(), item))
        .collect();
    let new_map: HashMap<&str, _> = new
        .iter()
        .map(|item| (item.material_no.as_str(), item))
        .collect();

    for item in new {
        match old_map.get(item.material_no.as_str()) {
            Some(old_item) => {
                if old_item.material_name != item.material_name {
                    changes.push(ChangeItem {
                        field: "物料名称".to_string(),
                        old_value: old_item.material_name.clone(),
                        new_value: item.material_name.clone(),
                        material_no: Some(item.material_no.clone()),
                    });
                }
                if old_item.specification != item.specification {
                    changes.push(ChangeItem {
                        field: "规格".to_string(),
                        old_value: old_item.specification.clone(),
                        new_value: item.specification.clone(),
                        material_no: Some(item.material_no.clone()),
                    });
                }
                if old_item.quantity != item.quantity {
                    changes.push(ChangeItem {
                        field: "数量".to_string(),
                        old_value: old_item.quantity.to_string(),
                        new_value: item.quantity.to_string(),
                        material_no: Some(item.material_no.clone()),
                    });
                }
                if old_item.loss_rate != item.loss_rate {
                    changes.push(ChangeItem {
                        field: "损耗率".to_string(),
                        old_value: old_item.loss_rate.to_string(),
                        new_value: item.loss_rate.to_string(),
                        material_no: Some(item.material_no.clone()),
                    });
                }
                if old_item.unit_price != item.unit_price {
                    changes.push(ChangeItem {
                        field: "单价".to_string(),
                        old_value: old_item.unit_price.to_string(),
                        new_value: item.unit_price.to_string(),
                        material_no: Some(item.material_no.clone()),
                    });
                }
                if old_item.supplier != item.supplier {
                    changes.push(ChangeItem {
                        field: "供应商".to_string(),
                        old_value: old_item.supplier.clone(),
                        new_value: item.supplier.clone(),
                        material_no: Some(item.material_no.clone()),
                    });
                }
                if old_item.remark != item.remark {
                    changes.push(ChangeItem {
                        field: "备注".to_string(),
                        old_value: old_item.remark.clone(),
                        new_value: item.remark.clone(),
                        material_no: Some(item.material_no.clone()),
                    });
                }
            }
            None => {
                changes.push(ChangeItem {
                    field: "新增物料".to_string(),
                    old_value: String::new(),
                    new_value: item.material_name.clone(),
                    material_no: Some(item.material_no.clone()),
                });
            }
        }
    }

    for item in old {
        if !new_map.contains_key(item.material_no.as_str()) {
            changes.push(ChangeItem {
                field: "删除物料".to_string(),
                old_value: item.material_name.clone(),
                new_value: String::new(),
                material_no: Some(item.material_no.clone()),
            });
        }
    }

    changes
}

pub struct VersionManager {
    histories: HashMap<String, VersionHistory>,
}

impl VersionManager {
    pub fn new() -> Self {
        VersionManager {
            histories: HashMap::new(),
        }
    }

    pub fn create_bom(&mut self, bom_id: &str, data: BomData, created_by: &str, reason: &str) -> &BomVersion {
        let history = VersionHistory::new(bom_id, data, created_by, reason);
        self.histories.insert(bom_id.to_string(), history);
        self.get_current(bom_id).unwrap()
    }

    pub fn update_bom(
        &mut self,
        bom_id: &str,
        data: BomData,
        modified_by: &str,
        reason: &str,
    ) -> Result<&BomVersion, String> {
        let history = self
            .histories
            .get_mut(bom_id)
            .ok_or_else(|| format!("BOM {} 不存在", bom_id))?;
        Ok(history.add_version(data, modified_by, reason))
    }

    pub fn rollback(
        &mut self,
        bom_id: &str,
        version_number: u32,
        modified_by: &str,
        reason: &str,
    ) -> Result<&BomVersion, String> {
        let history = self
            .histories
            .get_mut(bom_id)
            .ok_or_else(|| format!("BOM {} 不存在", bom_id))?;
        history.rollback(version_number, modified_by, reason)
    }

    pub fn get_current(&self, bom_id: &str) -> Option<&BomVersion> {
        self.histories.get(bom_id).map(|h| h.current())
    }

    pub fn get_version(&self, bom_id: &str, version_number: u32) -> Option<&BomVersion> {
        self.histories
            .get(bom_id)
            .and_then(|h| h.get_version(version_number))
    }

    pub fn list_versions(&self, bom_id: &str) -> Option<Vec<BomVersionSummary>> {
        self.histories.get(bom_id).map(|h| h.list_versions())
    }

    pub fn has_bom(&self, bom_id: &str) -> bool {
        self.histories.contains_key(bom_id)
    }

    pub fn all_bom_ids(&self) -> Vec<String> {
        self.histories.keys().cloned().collect()
    }
}

impl Default for VersionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bom::{BomItem, BomType, SourceType};

    fn sample_bom_data() -> BomData {
        vec![BomItem {
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
            remark: "初始版本".to_string(),
            parent_material_no: None,
        }]
    }

    fn sample_bom_data_v2() -> BomData {
        vec![BomItem {
            material_no: "A001".to_string(),
            material_name: "成品A-改进版".to_string(),
            specification: "100x100".to_string(),
            bom_type: BomType::FinishedProduct,
            quantity: 1.0,
            unit: "件".to_string(),
            loss_rate: 0.03,
            supplier: "".to_string(),
            unit_price: 120.0,
            source: SourceType::SelfMade,
            effective_date: "".to_string(),
            expiry_date: "".to_string(),
            remark: "改进版".to_string(),
            parent_material_no: None,
        }]
    }

    #[test]
    fn test_version_history_creation() {
        let data = sample_bom_data();
        let history = VersionHistory::new("bom-001", data, "工程师甲", "初始导入");

        assert_eq!(history.version_count(), 1);
        assert_eq!(history.current_version, 1);
        assert_eq!(history.current().modified_by, "工程师甲");
    }

    #[test]
    fn test_add_version_and_diff() {
        let data = sample_bom_data();
        let mut history = VersionHistory::new("bom-001", data, "工程师甲", "初始导入");

        let v2_data = sample_bom_data_v2();
        let v2 = history.add_version(v2_data, "工程师乙", "修改单价和损耗率");

        assert_eq!(v2.version_number, 2);
        assert!(v2.change_description.len() > 0);
        assert_eq!(history.current_version, 2);
    }

    #[test]
    fn test_rollback() {
        let data = sample_bom_data();
        let mut history = VersionHistory::new("bom-001", data, "工程师甲", "初始导入");

        let v2_data = sample_bom_data_v2();
        history.add_version(v2_data, "工程师乙", "修改单价");

        assert_eq!(history.current_version, 2);

        let v3 = history.rollback(1, "工程师丙", "发现新版算错了").unwrap();
        assert_eq!(v3.version_number, 3);
        assert!(v3.change_reason.contains("回滚"));
        assert_eq!(history.current().data[0].unit_price, 100.0);
    }

    #[test]
    fn test_version_manager() {
        let mut mgr = VersionManager::new();
        let data = sample_bom_data();

        mgr.create_bom("bom-001", data, "工程师甲", "初始导入");
        assert!(mgr.has_bom("bom-001"));
        assert!(!mgr.has_bom("bom-999"));

        let versions = mgr.list_versions("bom-001").unwrap();
        assert_eq!(versions.len(), 1);
    }

    #[test]
    fn test_diff_bom() {
        let old = sample_bom_data();
        let new = sample_bom_data_v2();

        let changes = diff_bom(&old, &new);
        assert!(changes.len() > 0);
    }
}
