use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BomError {
    #[error("CSV parse error: {0}")]
    CsvError(String),
    #[error("Excel parse error: {0}")]
    ExcelError(String),
    #[error("Invalid field: {0}")]
    InvalidField(String),
    #[error("Missing required field: {0}")]
    MissingField(String),
    #[error("Duplicate material number: {0}")]
    DuplicateMaterial(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BomType {
    FinishedProduct,
    SemiFinished,
    RawMaterial,
    Virtual,
}

impl BomType {
    pub fn from_str(s: &str) -> Result<Self, BomError> {
        match s.trim().to_lowercase().as_str() {
            "成品" | "finished_product" | "finished" => Ok(BomType::FinishedProduct),
            "半成品" | "semi_finished" | "semifinished" => Ok(BomType::SemiFinished),
            "原材料" | "raw_material" | "raw" => Ok(BomType::RawMaterial),
            "虚拟件" | "virtual" => Ok(BomType::Virtual),
            _ => Err(BomError::InvalidField(format!("未知类型: {}", s))),
        }
    }
}

impl fmt::Display for BomType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BomType::FinishedProduct => write!(f, "成品"),
            BomType::SemiFinished => write!(f, "半成品"),
            BomType::RawMaterial => write!(f, "原材料"),
            BomType::Virtual => write!(f, "虚拟件"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    SelfMade,
    Purchased,
    Outsourced,
}

impl SourceType {
    pub fn from_str(s: &str) -> Result<Self, BomError> {
        match s.trim().to_lowercase().as_str() {
            "自制" | "self_made" | "selfmade" => Ok(SourceType::SelfMade),
            "外购" | "purchased" | "buy" => Ok(SourceType::Purchased),
            "外协" | "outsourced" | "outsource" => Ok(SourceType::Outsourced),
            _ => Err(BomError::InvalidField(format!("未知来源: {}", s))),
        }
    }
}

impl fmt::Display for SourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SourceType::SelfMade => write!(f, "自制"),
            SourceType::Purchased => write!(f, "外购"),
            SourceType::Outsourced => write!(f, "外协"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BomItem {
    pub material_no: String,
    pub material_name: String,
    pub specification: String,
    pub bom_type: BomType,
    pub quantity: f64,
    pub unit: String,
    pub loss_rate: f64,
    pub supplier: String,
    pub unit_price: f64,
    pub source: SourceType,
    pub effective_date: String,
    pub expiry_date: String,
    pub remark: String,
    #[serde(default)]
    pub parent_material_no: Option<String>,
}

impl BomItem {
    pub fn is_virtual(&self) -> bool {
        matches!(self.bom_type, BomType::Virtual)
    }

    pub fn is_raw_material(&self) -> bool {
        matches!(self.bom_type, BomType::RawMaterial)
    }
}

pub type BomData = Vec<BomItem>;

pub mod parser {
    use super::*;
    use csv::ReaderBuilder;
    use std::io::Read;

    pub fn parse_csv(data: &[u8]) -> Result<BomData, BomError> {
        let mut rdr = ReaderBuilder::new()
            .has_headers(true)
            .flexible(true)
            .from_reader(data);

        let headers = rdr
            .headers()
            .map_err(|e| BomError::CsvError(e.to_string()))?
            .clone();

        let mut items = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for (idx, result) in rdr.records().enumerate() {
            let record = result.map_err(|e| BomError::CsvError(format!("行 {}: {}", idx + 2, e)))?;
            let item = parse_csv_row(&record, &headers, idx + 2)?;

            if !seen.insert(item.material_no.clone()) {
                return Err(BomError::DuplicateMaterial(item.material_no.clone()));
            }

            items.push(item);
        }

        Ok(items)
    }

    fn parse_csv_row(
        record: &csv::StringRecord,
        headers: &csv::StringRecord,
        line_no: usize,
    ) -> Result<BomItem, BomError> {
        let get = |name: &str| -> Result<&str, BomError> {
            headers
                .iter()
                .position(|h| h.trim().eq_ignore_ascii_case(name.trim()))
                .and_then(|i| record.get(i))
                .map(|s| s.trim())
                .ok_or_else(|| BomError::MissingField(format!("{} (行 {})", name, line_no)))
        };

        let get_opt = |name: &str| -> Option<&str> {
            headers
                .iter()
                .position(|h| h.trim().eq_ignore_ascii_case(name.trim()))
                .and_then(|i| record.get(i))
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
        };

        let material_no = get("物料号")?.to_string();
        let material_name = get("物料名称")?.to_string();
        let specification = get_opt("规格").unwrap_or("").to_string();
        let bom_type = BomType::from_str(get("类型")?)?;
        let quantity: f64 = get("数量")?
            .parse()
            .map_err(|_| BomError::InvalidField(format!("数量格式错误 (行 {})", line_no)))?;
        let unit = get("单位")?.to_string();
        let loss_rate: f64 = get_opt("损耗率")
            .unwrap_or("0")
            .parse()
            .map_err(|_| BomError::InvalidField(format!("损耗率格式错误 (行 {})", line_no)))?;
        let supplier = get_opt("供应商").unwrap_or("").to_string();
        let unit_price: f64 = get_opt("单价")
            .unwrap_or("0")
            .parse()
            .map_err(|_| BomError::InvalidField(format!("单价格式错误 (行 {})", line_no)))?;
        let source = SourceType::from_str(get_opt("来源").unwrap_or("自制"))?;
        let effective_date = get_opt("生效日期").unwrap_or("").to_string();
        let expiry_date = get_opt("失效日期").unwrap_or("").to_string();
        let remark = get_opt("备注").unwrap_or("").to_string();
        let parent_material_no = get_opt("父件物料号").map(|s| s.to_string());

        Ok(BomItem {
            material_no,
            material_name,
            specification,
            bom_type,
            quantity,
            unit,
            loss_rate,
            supplier,
            unit_price,
            source,
            effective_date,
            expiry_date,
            remark,
            parent_material_no,
        })
    }

    pub fn parse_excel(data: &[u8]) -> Result<BomData, BomError> {
        use calamine::{DataType, Reader};

        let cursor = std::io::Cursor::new(data);
        let mut workbook = calamine::open_workbook_auto_from_rs(cursor)
            .map_err(|e| BomError::ExcelError(e.to_string()))?;

        let sheet_name = workbook
            .sheet_names()
            .first()
            .ok_or_else(|| BomError::ExcelError("Excel 文件中没有工作表".to_string()))?
            .clone();

        let range = workbook
            .worksheet_range(&sheet_name)
            .map_err(|e| BomError::ExcelError(e.to_string()))?;

        if range.is_empty() {
            return Err(BomError::ExcelError("工作表为空".to_string()));
        }

        let rows: Vec<Vec<String>> = range
            .rows()
            .map(|row| {
                row.iter()
                    .map(|cell| cell.get_string().unwrap_or("").to_string())
                    .collect()
            })
            .collect();

        if rows.len() < 2 {
            return Err(BomError::ExcelError("工作表至少需要表头和一行数据".to_string()));
        }

        let headers: Vec<String> = rows[0].iter().map(|h| h.trim().to_string()).collect();
        let mut items = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for (row_idx, row) in rows.iter().skip(1).enumerate() {
            let item = parse_excel_row(row, &headers, row_idx + 2)?;

            if item.material_no.is_empty() {
                continue;
            }

            if !seen.insert(item.material_no.clone()) {
                return Err(BomError::DuplicateMaterial(item.material_no.clone()));
            }

            items.push(item);
        }

        Ok(items)
    }

    fn parse_excel_row(
        row: &[String],
        headers: &[String],
        line_no: usize,
    ) -> Result<BomItem, BomError> {
        let get = |name: &str| -> Result<&str, BomError> {
            headers
                .iter()
                .position(|h| h.trim().eq_ignore_ascii_case(name.trim()))
                .and_then(|i| row.get(i))
                .map(|s| s.trim())
                .ok_or_else(|| BomError::MissingField(format!("{} (行 {})", name, line_no)))
        };

        let get_opt = |name: &str| -> Option<&str> {
            headers
                .iter()
                .position(|h| h.trim().eq_ignore_ascii_case(name.trim()))
                .and_then(|i| row.get(i))
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
        };

        let material_no = get("物料号")?.to_string();
        if material_no.is_empty() {
            return Ok(BomItem {
                material_no: String::new(),
                material_name: String::new(),
                specification: String::new(),
                bom_type: BomType::RawMaterial,
                quantity: 0.0,
                unit: String::new(),
                loss_rate: 0.0,
                supplier: String::new(),
                unit_price: 0.0,
                source: SourceType::SelfMade,
                effective_date: String::new(),
                expiry_date: String::new(),
                remark: String::new(),
                parent_material_no: None,
            });
        }

        let material_name = get("物料名称")?.to_string();
        let specification = get_opt("规格").unwrap_or("").to_string();
        let bom_type = BomType::from_str(get("类型")?)?;
        let quantity: f64 = get("数量")?
            .parse()
            .map_err(|_| BomError::InvalidField(format!("数量格式错误 (行 {})", line_no)))?;
        let unit = get("单位")?.to_string();
        let loss_rate: f64 = get_opt("损耗率")
            .unwrap_or("0")
            .parse()
            .map_err(|_| BomError::InvalidField(format!("损耗率格式错误 (行 {})", line_no)))?;
        let supplier = get_opt("供应商").unwrap_or("").to_string();
        let unit_price: f64 = get_opt("单价")
            .unwrap_or("0")
            .parse()
            .map_err(|_| BomError::InvalidField(format!("单价格式错误 (行 {})", line_no)))?;
        let source = SourceType::from_str(get_opt("来源").unwrap_or("自制"))?;
        let effective_date = get_opt("生效日期").unwrap_or("").to_string();
        let expiry_date = get_opt("失效日期").unwrap_or("").to_string();
        let remark = get_opt("备注").unwrap_or("").to_string();
        let parent_material_no = get_opt("父件物料号").map(|s| s.to_string());

        Ok(BomItem {
            material_no,
            material_name,
            specification,
            bom_type,
            quantity,
            unit,
            loss_rate,
            supplier,
            unit_price,
            source,
            effective_date,
            expiry_date,
            remark,
            parent_material_no,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_parse_csv_basic() {
            let csv_data = "\
物料号,物料名称,规格,类型,数量,单位,损耗率,供应商,单价,来源,生效日期,失效日期,备注,父件物料号
A001,成品A,100x100,成品,1,件,0.05,供应商A,100,自制,2024-01-01,2025-12-31,主产品,
B001,半成品B,50x50,半成品,1,件,0.03,供应商B,50,自制,2024-01-01,,子件,A001
";
            let items = parse_csv(csv_data.as_bytes()).unwrap();
            assert_eq!(items.len(), 2);
            assert_eq!(items[0].material_no, "A001");
            assert_eq!(items[0].bom_type, BomType::FinishedProduct);
            assert_eq!(items[1].parent_material_no.as_deref(), Some("A001"));
        }

        #[test]
        fn test_bom_type_from_str() {
            assert_eq!(BomType::from_str("成品").unwrap(), BomType::FinishedProduct);
            assert_eq!(BomType::from_str("半成品").unwrap(), BomType::SemiFinished);
            assert_eq!(BomType::from_str("原材料").unwrap(), BomType::RawMaterial);
            assert_eq!(BomType::from_str("虚拟件").unwrap(), BomType::Virtual);
        }
    }
}
