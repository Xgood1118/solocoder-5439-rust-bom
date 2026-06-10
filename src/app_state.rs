use crate::alt::AltMaterialManager;
use crate::bom::{parser, BomData, BomError};
use crate::calc::{CostCalculator, CostCalculationResult, PriceTiersConfig, SurchargeConfig};
use crate::tree::{self, BomTreeNode};
use crate::validation::{BomValidator, ValidationResult};
use crate::version::{BomVersion, VersionManager};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use uuid::Uuid;

pub struct AppState {
    version_manager: RwLock<VersionManager>,
    alt_manager: Arc<AltMaterialManager>,
    validator: RwLock<BomValidator>,
    cost_calculator: RwLock<CostCalculator>,
    batch_jobs: RwLock<HashMap<String, BatchJob>>,
}

impl AppState {
    pub fn new() -> Self {
        AppState {
            version_manager: RwLock::new(VersionManager::new()),
            alt_manager: Arc::new(AltMaterialManager::new()),
            validator: RwLock::new(BomValidator::new()),
            cost_calculator: RwLock::new(CostCalculator::new()),
            batch_jobs: RwLock::new(HashMap::new()),
        }
    }

    pub fn alt_manager(&self) -> Arc<AltMaterialManager> {
        self.alt_manager.clone()
    }

    pub fn parse_bom(&self, data: &[u8], file_type: &str) -> Result<BomData, BomError> {
        match file_type.to_lowercase().as_str() {
            "csv" => parser::parse_csv(data),
            "xlsx" | "xls" | "excel" => parser::parse_excel(data),
            _ => Err(BomError::InvalidField(format!(
                "不支持的文件类型: {}",
                file_type
            ))),
        }
    }

    pub fn create_bom(
        &self,
        bom_id: Option<&str>,
        data: BomData,
        created_by: &str,
        reason: &str,
    ) -> BomVersion {
        let mut vm = self.version_manager.write();
        let id = bom_id.unwrap_or(&format!("bom-{}", Uuid::new_v4())).to_string();
        vm.create_bom(&id, data, created_by, reason).clone()
    }

    pub fn update_bom(
        &self,
        bom_id: &str,
        data: BomData,
        modified_by: &str,
        reason: &str,
    ) -> Result<BomVersion, String> {
        let mut vm = self.version_manager.write();
        vm.update_bom(bom_id, data, modified_by, reason)
            .map(|v| v.clone())
    }

    pub fn get_bom(&self, bom_id: &str) -> Option<BomData> {
        let vm = self.version_manager.read();
        vm.get_current(bom_id).map(|v| v.data.clone())
    }

    pub fn get_bom_tree(&self, bom_id: &str) -> Option<Vec<BomTreeNode>> {
        self.get_bom(bom_id).map(|data| tree::build_tree(&data))
    }

    pub fn get_bom_version(&self, bom_id: &str, version: u32) -> Option<BomVersion> {
        let vm = self.version_manager.read();
        vm.get_version(bom_id, version).cloned()
    }

    pub fn list_bom_versions(&self, bom_id: &str) -> Option<Vec<crate::version::BomVersionSummary>> {
        let vm = self.version_manager.read();
        vm.list_versions(bom_id)
    }

    pub fn rollback_bom(
        &self,
        bom_id: &str,
        version: u32,
        modified_by: &str,
        reason: &str,
    ) -> Result<BomVersion, String> {
        let mut vm = self.version_manager.write();
        vm.rollback(bom_id, version, modified_by, reason)
            .map(|v| v.clone())
    }

    pub fn list_boms(&self) -> Vec<String> {
        let vm = self.version_manager.read();
        vm.all_bom_ids()
    }

    pub fn validate_bom(&self, bom_id: &str) -> Option<ValidationResult> {
        let data = self.get_bom(bom_id)?;
        let validator = self.validator.read();
        Some(validator.validate(&data))
    }

    pub fn validate_data(&self, data: &BomData) -> ValidationResult {
        let validator = self.validator.read();
        validator.validate(data)
    }

    pub fn calculate_cost(&self, bom_id: &str, quantity: f64) -> Option<CostCalculationResult> {
        let tree = self.get_bom_tree(bom_id)?;
        let calc = self.cost_calculator.read();
        Some(calc.calculate(&tree, quantity))
    }

    pub fn set_price_tiers(&self, tiers: PriceTiersConfig) {
        let mut calc = self.cost_calculator.write();
        *calc = CostCalculator::new()
            .with_price_tiers(tiers)
            .with_surcharge(calc.surcharge_config().clone());
    }

    pub fn set_surcharge_config(&self, surcharge: SurchargeConfig) {
        let mut calc = self.cost_calculator.write();
        *calc = CostCalculator::new()
            .with_price_tiers(calc.price_tiers().clone())
            .with_surcharge(surcharge);
    }

    pub fn load_master_data(&self, material_nos: Vec<String>) {
        let mut validator = self.validator.write();
        validator.load_master_data(material_nos);
    }

    pub fn start_batch_job(&self, total: usize) -> String {
        let job_id = Uuid::new_v4().to_string();
        let job = BatchJob {
            id: job_id.clone(),
            total,
            completed: AtomicUsize::new(0),
            failed: AtomicUsize::new(0),
            status: BatchStatus::Running,
            results: RwLock::new(Vec::new()),
            errors: RwLock::new(Vec::new()),
        };

        let mut jobs = self.batch_jobs.write();
        jobs.insert(job_id.clone(), job);

        job_id
    }

    pub fn update_batch_progress(&self, job_id: &str, success: bool, result: BatchResultItem) {
        let jobs = self.batch_jobs.read();
        if let Some(job) = jobs.get(job_id) {
            if success {
                job.completed.fetch_add(1, Ordering::SeqCst);
                job.results.write().push(result);
            } else {
                job.failed.fetch_add(1, Ordering::SeqCst);
                job.errors.write().push(result.error.unwrap_or_default());
            }
        }
    }

    pub fn complete_batch_job(&self, job_id: &str) {
        let mut jobs = self.batch_jobs.write();
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = BatchStatus::Completed;
        }
    }

    pub fn fail_batch_job(&self, job_id: &str) {
        let mut jobs = self.batch_jobs.write();
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = BatchStatus::Failed;
        }
    }

    pub fn get_batch_progress(&self, job_id: &str) -> Option<BatchProgress> {
        let jobs = self.batch_jobs.read();
        jobs.get(job_id).map(|job| BatchProgress {
            id: job.id.clone(),
            total: job.total,
            completed: job.completed.load(Ordering::SeqCst),
            failed: job.failed.load(Ordering::SeqCst),
            status: job.status.clone(),
            percentage: if job.total > 0 {
                (job.completed.load(Ordering::SeqCst) + job.failed.load(Ordering::SeqCst)) as f64
                    / job.total as f64
                    * 100.0
            } else {
                0.0
            },
        })
    }

    pub fn get_batch_result(&self, job_id: &str) -> Option<BatchJobResult> {
        let jobs = self.batch_jobs.read();
        jobs.get(job_id).map(|job| BatchJobResult {
            id: job.id.clone(),
            total: job.total,
            completed: job.completed.load(Ordering::SeqCst),
            failed: job.failed.load(Ordering::SeqCst),
            status: job.status.clone(),
            results: job.results.read().clone(),
            errors: job.errors.read().clone(),
        })
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct BatchJob {
    pub id: String,
    pub total: usize,
    pub completed: AtomicUsize,
    pub failed: AtomicUsize,
    pub status: BatchStatus,
    pub results: RwLock<Vec<BatchResultItem>>,
    pub errors: RwLock<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchStatus {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResultItem {
    pub file_name: String,
    pub bom_id: Option<String>,
    pub item_count: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchProgress {
    pub id: String,
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub status: BatchStatus,
    pub percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchJobResult {
    pub id: String,
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub status: BatchStatus,
    pub results: Vec<BatchResultItem>,
    pub errors: Vec<String>,
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
            remark: "".to_string(),
            parent_material_no: None,
        }]
    }

    #[test]
    fn test_create_and_get_bom() {
        let state = AppState::new();
        let data = sample_bom_data();
        let version = state.create_bom(Some("bom-001"), data, "测试员", "测试");

        assert_eq!(version.version_number, 1);
        assert!(state.get_bom("bom-001").is_some());
    }

    #[test]
    fn test_validate_bom() {
        let state = AppState::new();
        let data = sample_bom_data();
        state.create_bom(Some("bom-001"), data, "测试员", "测试");

        let result = state.validate_bom("bom-001");
        assert!(result.is_some());
        assert!(result.unwrap().valid);
    }

    #[test]
    fn test_calculate_cost() {
        let state = AppState::new();
        let data = sample_bom_data();
        state.create_bom(Some("bom-001"), data, "测试员", "测试");

        let result = state.calculate_cost("bom-001", 100.0);
        assert!(result.is_some());
        assert!(result.unwrap().total_cost > 0.0);
    }

    #[test]
    fn test_batch_job_progress() {
        let state = AppState::new();
        let job_id = state.start_batch_job(10);

        let progress = state.get_batch_progress(&job_id).unwrap();
        assert_eq!(progress.total, 10);
        assert_eq!(progress.completed, 0);

        state.update_batch_progress(
            &job_id,
            true,
            BatchResultItem {
                file_name: "test.csv".to_string(),
                bom_id: Some("bom-001".to_string()),
                item_count: 10,
                error: None,
            },
        );

        let progress2 = state.get_batch_progress(&job_id).unwrap();
        assert_eq!(progress2.completed, 1);
        assert_eq!(progress2.percentage, 10.0);

        state.complete_batch_job(&job_id);
        let progress3 = state.get_batch_progress(&job_id).unwrap();
        assert_eq!(progress3.status, BatchStatus::Completed);
    }

    #[test]
    fn test_version_rollback() {
        let state = AppState::new();
        let data = sample_bom_data();
        state.create_bom(Some("bom-001"), data, "测试员", "v1");

        let mut data2 = sample_bom_data();
        data2[0].unit_price = 200.0;
        state
            .update_bom("bom-001", data2, "测试员", "v2 修改单价")
            .unwrap();

        let versions = state.list_bom_versions("bom-001").unwrap();
        assert_eq!(versions.len(), 2);

        let v3 = state
            .rollback_bom("bom-001", 1, "测试员", "回滚")
            .unwrap();
        assert_eq!(v3.version_number, 3);
        assert_eq!(state.get_bom("bom-001").unwrap()[0].unit_price, 100.0);
    }
}
