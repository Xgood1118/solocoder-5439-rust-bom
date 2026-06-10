use crate::alt::AltMaterialGroup;
use crate::app_state::{
    AppState, BatchResultItem,
};
use crate::bom::BomData;
use crate::calc::{PriceTiersConfig, SurchargeConfig};

use actix_web::{web, HttpResponse, Responder};
use actix_multipart::Multipart;
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api")
            .service(
                web::scope("/bom")
                    .route("/upload", web::post().to(upload_bom))
                    .route("", web::get().to(list_boms))
                    .route("/{id}", web::get().to(get_bom))
                    .route("/{id}/tree", web::get().to(get_bom_tree))
                    .route("/{id}", web::put().to(update_bom))
                    .route("/{id}/validate", web::get().to(validate_bom))
                    .route("/{id}/cost", web::get().to(calculate_cost))
                    .service(
                        web::scope("/{id}/versions")
                            .route("", web::get().to(list_versions))
                            .route("/{version}", web::get().to(get_version))
                            .route("/{version}/rollback", web::post().to(rollback_version)),
                    ),
            )
            .service(
                web::scope("/batch")
                    .route("/upload", web::post().to(batch_upload))
                    .route("/{job_id}/progress", web::get().to(batch_progress))
                    .route("/{job_id}/result", web::get().to(batch_result)),
            )
            .service(
                web::scope("/config")
                    .route("/price-tiers", web::get().to(get_price_tiers))
                    .route("/price-tiers", web::put().to(set_price_tiers))
                    .route("/surcharge", web::get().to(get_surcharge))
                    .route("/surcharge", web::put().to(set_surcharge))
                    .route("/master-data", web::post().to(load_master_data)),
            )
            .service(
                web::scope("/alt")
                    .route("", web::get().to(list_alt_groups))
                    .route("", web::post().to(add_alt_group))
                    .route("/{material_no}", web::get().to(get_alt_group))
                    .route("/{material_no}/current", web::get().to(get_current_alt))
                    .route("/{material_no}/switch", web::post().to(switch_alt))
                    .route("/{material_no}/reset", web::post().to(reset_alt))
                    .route("/{material_no}/stock", web::put().to(update_stock)),
            ),
    )
    .route("/health", web::get().to(health_check));
}

async fn health_check() -> impl Responder {
    HttpResponse::Ok().json(serde_json::json!({ "status": "ok" }))
}

#[derive(Debug, Deserialize)]
struct UploadQuery {
    bom_id: Option<String>,
    created_by: Option<String>,
    reason: Option<String>,
}

async fn upload_bom(
    state: web::Data<AppState>,
    query: web::Query<UploadQuery>,
    mut payload: Multipart,
) -> impl Responder {
    let mut file_data: Option<Vec<u8>> = None;
    let mut file_name = String::new();
    let mut file_type = String::new();

    while let Some(item) = payload.next().await {
        let mut field = match item {
            Ok(f) => f,
            Err(e) => {
                return HttpResponse::BadRequest().json(
                    serde_json::json!({ "error": format!("上传错误: {}", e) })
                );
            }
        };

        let content_disposition = field.content_disposition();
        let name = content_disposition.get_name().unwrap_or("");

        if name == "file" {
            if let Some(filename) = content_disposition.get_filename() {
                file_name = filename.to_string();
                let ext = Path::new(filename)
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();
                file_type = ext;
            }

            let mut bytes = Vec::new();
            while let Some(chunk) = field.next().await {
                match chunk {
                    Ok(data) => bytes.extend_from_slice(data.as_ref()),
                    Err(e) => {
                        return HttpResponse::BadRequest().json(
                            serde_json::json!({ "error": format!("读取文件错误: {}", e) })
                        );
                    }
                }
            }
            file_data = Some(bytes);
        }
    }

    let data = match file_data {
        Some(d) => d,
        None => {
            return HttpResponse::BadRequest().json(
                serde_json::json!({ "error": "未找到上传文件" })
            );
        }
    };

    let bom_data = match state.parse_bom(&data, &file_type) {
        Ok(d) => d,
        Err(e) => {
            return HttpResponse::BadRequest().json(
                serde_json::json!({ "error": format!("解析失败: {}", e) })
            );
        }
    };

    let validation = state.validate_data(&bom_data);
    if !validation.valid {
        return HttpResponse::UnprocessableEntity().json(serde_json::json!({
            "error": "BOM 校验未通过，拒绝入库",
            "file_name": file_name,
            "item_count": bom_data.len(),
            "validation": validation,
        }));
    }

    let created_by = query.created_by.as_deref().unwrap_or("system");
    let reason = query.reason.as_deref().unwrap_or("上传导入");

    let version = state.create_bom(
        query.bom_id.as_deref(),
        bom_data,
        created_by,
        reason,
    );

    HttpResponse::Ok().json(serde_json::json!({
        "bom_id": version.bom_id,
        "version": version.version_number,
        "item_count": version.data.len(),
        "file_name": file_name,
        "validation": {
            "valid": true,
            "error_count": 0,
            "warning_count": validation.warning_count,
        },
    }))
}

async fn list_boms(state: web::Data<AppState>) -> impl Responder {
    let boms = state.list_boms();
    HttpResponse::Ok().json(serde_json::json!({ "boms": boms }))
}

async fn get_bom(state: web::Data<AppState>, id: web::Path<String>) -> impl Responder {
    match state.get_bom(&id) {
        Some(data) => HttpResponse::Ok().json(serde_json::json!({
            "bom_id": id.into_inner(),
            "items": data,
        })),
        None => HttpResponse::NotFound().json(serde_json::json!({ "error": "BOM 不存在" })),
    }
}

async fn get_bom_tree(state: web::Data<AppState>, id: web::Path<String>) -> impl Responder {
    match state.get_bom_tree(&id) {
        Some(tree) => {
            let max_depth = crate::tree::max_depth(&tree);
            let node_count = crate::tree::count_nodes(&tree);
            HttpResponse::Ok().json(serde_json::json!({
                "bom_id": id.into_inner(),
                "tree": tree,
                "max_depth": max_depth,
                "node_count": node_count,
            }))
        }
        None => HttpResponse::NotFound().json(serde_json::json!({ "error": "BOM 不存在" })),
    }
}

#[derive(Debug, Deserialize)]
struct UpdateBomRequest {
    items: BomData,
    modified_by: String,
    reason: String,
}

async fn update_bom(
    state: web::Data<AppState>,
    id: web::Path<String>,
    body: web::Json<UpdateBomRequest>,
) -> impl Responder {
    match state.update_bom(&id, body.items.clone(), &body.modified_by, &body.reason) {
        Ok(version) => HttpResponse::Ok().json(serde_json::json!({
            "bom_id": version.bom_id,
            "version": version.version_number,
            "change_count": version.change_description.len(),
        })),
        Err(e) => HttpResponse::NotFound().json(serde_json::json!({ "error": e })),
    }
}

async fn validate_bom(state: web::Data<AppState>, id: web::Path<String>) -> impl Responder {
    match state.validate_bom(&id) {
        Some(result) => HttpResponse::Ok().json(result),
        None => HttpResponse::NotFound().json(serde_json::json!({ "error": "BOM 不存在" })),
    }
}

#[derive(Debug, Deserialize)]
struct CostQuery {
    quantity: f64,
}

async fn calculate_cost(
    state: web::Data<AppState>,
    id: web::Path<String>,
    query: web::Query<CostQuery>,
) -> impl Responder {
    if query.quantity <= 0.0 {
        return HttpResponse::BadRequest().json(
            serde_json::json!({ "error": "数量必须为正数" })
        );
    }

    match state.calculate_cost(&id, query.quantity) {
        Some(result) => HttpResponse::Ok().json(result),
        None => HttpResponse::NotFound().json(serde_json::json!({ "error": "BOM 不存在" })),
    }
}

async fn list_versions(state: web::Data<AppState>, id: web::Path<String>) -> impl Responder {
    match state.list_bom_versions(&id) {
        Some(versions) => HttpResponse::Ok().json(serde_json::json!({
            "bom_id": id.into_inner(),
            "versions": versions,
        })),
        None => HttpResponse::NotFound().json(serde_json::json!({ "error": "BOM 不存在" })),
    }
}

async fn get_version(
    state: web::Data<AppState>,
    path: web::Path<(String, u32)>,
) -> impl Responder {
    let (bom_id, version) = path.into_inner();
    match state.get_bom_version(&bom_id, version) {
        Some(v) => HttpResponse::Ok().json(v),
        None => HttpResponse::NotFound().json(serde_json::json!({ "error": "版本不存在" })),
    }
}

#[derive(Debug, Deserialize)]
struct RollbackRequest {
    modified_by: String,
    reason: String,
}

async fn rollback_version(
    state: web::Data<AppState>,
    path: web::Path<(String, u32)>,
    body: web::Json<RollbackRequest>,
) -> impl Responder {
    let (bom_id, version) = path.into_inner();
    match state.rollback_bom(&bom_id, version, &body.modified_by, &body.reason) {
        Ok(v) => HttpResponse::Ok().json(serde_json::json!({
            "bom_id": v.bom_id,
            "new_version": v.version_number,
            "rolled_back_to": version,
        })),
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({ "error": e })),
    }
}

#[derive(Debug, Deserialize)]
struct BatchUploadQuery {
    created_by: Option<String>,
}

async fn batch_upload(
    state: web::Data<AppState>,
    query: web::Query<BatchUploadQuery>,
    mut payload: Multipart,
) -> impl Responder {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();

    while let Some(item) = payload.next().await {
        let mut field = match item {
            Ok(f) => f,
            Err(e) => {
                return HttpResponse::BadRequest().json(
                    serde_json::json!({ "error": format!("上传错误: {}", e) })
                );
            }
        };

        let content_disposition = field.content_disposition();
        let name = content_disposition.get_name().unwrap_or("");

        if name == "files" || name == "file" {
            let file_name = content_disposition
                .get_filename()
                .unwrap_or("unknown")
                .to_string();

            let mut bytes = Vec::new();
            while let Some(chunk) = field.next().await {
                match chunk {
                    Ok(data) => bytes.extend_from_slice(data.as_ref()),
                    Err(e) => {
                        return HttpResponse::BadRequest().json(
                            serde_json::json!({ "error": format!("读取文件错误: {}", e) })
                        );
                    }
                }
            }
            files.push((file_name, bytes));
        }
    }

    if files.is_empty() {
        return HttpResponse::BadRequest().json(
            serde_json::json!({ "error": "未找到上传文件" })
        );
    }

    let total = files.len();
    let job_id = state.start_batch_job(total);
    let created_by = query.created_by.as_deref().unwrap_or("system").to_string();
    let state_clone = state.clone();
    let job_id_clone = job_id.clone();

    tokio::spawn(async move {
        for (file_name, data) in files {
            let ext = Path::new(&file_name)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            let parse_result = state_clone.parse_bom(&data, &ext);
            match parse_result {
                Ok(bom_data) => {
                    let item_count = bom_data.len();
                    let validation = state_clone.validate_data(&bom_data);

                    if !validation.valid {
                        let err_msg = format!(
                            "校验未通过: {} 个错误, {} 个警告",
                            validation.error_count, validation.warning_count
                        );
                        state_clone.update_batch_progress(
                            &job_id_clone,
                            false,
                            BatchResultItem {
                                file_name: file_name.clone(),
                                bom_id: None,
                                item_count,
                                error: Some(err_msg),
                                validation: Some(validation),
                            },
                        );
                    } else {
                        let bom_id = format!("bom-{}", uuid::Uuid::new_v4());
                        state_clone.create_bom(
                            Some(&bom_id),
                            bom_data,
                            &created_by,
                            &format!("批量导入: {}", file_name),
                        );
                        state_clone.update_batch_progress(
                            &job_id_clone,
                            true,
                            BatchResultItem {
                                file_name: file_name.clone(),
                                bom_id: Some(bom_id),
                                item_count,
                                error: None,
                                validation: Some(validation),
                            },
                        );
                    }
                }
                Err(e) => {
                    state_clone.update_batch_progress(
                        &job_id_clone,
                        false,
                        BatchResultItem {
                            file_name: file_name.clone(),
                            bom_id: None,
                            item_count: 0,
                            error: Some(e.to_string()),
                            validation: None,
                        },
                    );
                }
            }
        }
        state_clone.complete_batch_job(&job_id_clone);
    });

    HttpResponse::Ok().json(serde_json::json!({
        "job_id": job_id,
        "total_files": total,
        "status": "running",
    }))
}

async fn batch_progress(
    state: web::Data<AppState>,
    job_id: web::Path<String>,
) -> impl Responder {
    match state.get_batch_progress(&job_id) {
        Some(progress) => HttpResponse::Ok().json(progress),
        None => HttpResponse::NotFound().json(serde_json::json!({ "error": "任务不存在" })),
    }
}

async fn batch_result(
    state: web::Data<AppState>,
    job_id: web::Path<String>,
) -> impl Responder {
    match state.get_batch_result(&job_id) {
        Some(result) => HttpResponse::Ok().json(result),
        None => HttpResponse::NotFound().json(serde_json::json!({ "error": "任务不存在" })),
    }
}

async fn get_price_tiers() -> impl Responder {
    let tiers = PriceTiersConfig::default();
    HttpResponse::Ok().json(tiers)
}

async fn set_price_tiers(
    state: web::Data<AppState>,
    body: web::Json<PriceTiersConfig>,
) -> impl Responder {
    state.set_price_tiers(body.into_inner());
    HttpResponse::Ok().json(serde_json::json!({ "status": "ok" }))
}

async fn get_surcharge() -> impl Responder {
    let surcharge = SurchargeConfig::default();
    HttpResponse::Ok().json(surcharge)
}

async fn set_surcharge(
    state: web::Data<AppState>,
    body: web::Json<SurchargeConfig>,
) -> impl Responder {
    state.set_surcharge_config(body.into_inner());
    HttpResponse::Ok().json(serde_json::json!({ "status": "ok" }))
}

#[derive(Debug, Deserialize)]
struct MasterDataRequest {
    material_nos: Vec<String>,
}

async fn load_master_data(
    state: web::Data<AppState>,
    body: web::Json<MasterDataRequest>,
) -> impl Responder {
    state.load_master_data(body.material_nos.clone());
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "count": body.material_nos.len(),
    }))
}

async fn list_alt_groups(state: web::Data<AppState>) -> impl Responder {
    let groups = state.alt_manager().all_groups();
    HttpResponse::Ok().json(serde_json::json!({ "groups": groups }))
}

async fn add_alt_group(
    state: web::Data<AppState>,
    body: web::Json<AltMaterialGroup>,
) -> impl Responder {
    state.alt_manager().add_group(body.into_inner());
    HttpResponse::Ok().json(serde_json::json!({ "status": "ok" }))
}

async fn get_alt_group(
    state: web::Data<AppState>,
    material_no: web::Path<String>,
) -> impl Responder {
    match state.alt_manager().get_group(&material_no) {
        Some(group) => HttpResponse::Ok().json(group),
        None => HttpResponse::NotFound().json(serde_json::json!({ "error": "替代料组不存在" })),
    }
}

async fn get_current_alt(
    state: web::Data<AppState>,
    material_no: web::Path<String>,
) -> impl Responder {
    match state.alt_manager().get_current_alt(&material_no) {
        Some(alt) => HttpResponse::Ok().json(alt),
        None => HttpResponse::NotFound().json(serde_json::json!({ "error": "替代料组不存在" })),
    }
}

#[derive(Debug, Deserialize)]
struct SwitchAltRequest {
    required_qty: f64,
}

async fn switch_alt(
    state: web::Data<AppState>,
    material_no: web::Path<String>,
    body: web::Json<SwitchAltRequest>,
) -> impl Responder {
    match state.alt_manager().try_switch_alt(&material_no, body.required_qty) {
        Ok(alt) => HttpResponse::Ok().json(serde_json::json!({
            "status": "switched",
            "supplier": alt.supplier,
            "material_no": alt.material_no,
        })),
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({ "error": e })),
    }
}

async fn reset_alt(
    state: web::Data<AppState>,
    material_no: web::Path<String>,
) -> impl Responder {
    match state.alt_manager().reset_alt(&material_no) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({ "status": "ok" })),
        Err(e) => HttpResponse::NotFound().json(serde_json::json!({ "error": e })),
    }
}

#[derive(Debug, Deserialize)]
struct UpdateStockRequest {
    supplier: String,
    new_stock: f64,
}

async fn update_stock(
    state: web::Data<AppState>,
    material_no: web::Path<String>,
    body: web::Json<UpdateStockRequest>,
) -> impl Responder {
    match state.alt_manager().update_stock(&material_no, &body.supplier, body.new_stock) {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({
            "status": "ok",
            "supplier": body.supplier,
            "new_stock": body.new_stock,
        })),
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({ "error": e })),
    }
}
