use crate::bom::{BomData, BomItem, BomType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BomTreeNode {
    pub material_no: String,
    pub material_name: String,
    pub specification: String,
    pub bom_type: BomType,
    pub quantity: f64,
    pub unit: String,
    pub loss_rate: f64,
    pub supplier: String,
    pub unit_price: f64,
    pub source: crate::bom::SourceType,
    pub effective_date: String,
    pub expiry_date: String,
    pub remark: String,
    pub children: Vec<BomTreeNode>,
    pub level: usize,
    pub path: String,
}

impl BomTreeNode {
    pub fn from_item(item: &BomItem) -> Self {
        BomTreeNode {
            material_no: item.material_no.clone(),
            material_name: item.material_name.clone(),
            specification: item.specification.clone(),
            bom_type: item.bom_type,
            quantity: item.quantity,
            unit: item.unit.clone(),
            loss_rate: item.loss_rate,
            supplier: item.supplier.clone(),
            unit_price: item.unit_price,
            source: item.source,
            effective_date: item.effective_date.clone(),
            expiry_date: item.expiry_date.clone(),
            remark: item.remark.clone(),
            children: Vec::new(),
            level: 0,
            path: String::new(),
        }
    }

    pub fn is_virtual(&self) -> bool {
        matches!(self.bom_type, BomType::Virtual)
    }

    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }
}

pub fn build_tree(items: &BomData) -> Vec<BomTreeNode> {
    let mut index: HashMap<String, BomTreeNode> = HashMap::new();
    let mut children_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut root_candidates: Vec<String> = Vec::new();

    for item in items {
        let node = BomTreeNode::from_item(item);
        index.insert(item.material_no.clone(), node);

        if let Some(parent) = &item.parent_material_no {
            if !parent.is_empty() {
                children_map
                    .entry(parent.clone())
                    .or_insert_with(Vec::new)
                    .push(item.material_no.clone());
                continue;
            }
        }
        root_candidates.push(item.material_no.clone());
    }

    for item in items {
        if let Some(parent) = &item.parent_material_no {
            if !parent.is_empty() && !index.contains_key(parent) {
                root_candidates.push(item.material_no.clone());
            }
        }
    }

    let mut roots: Vec<BomTreeNode> = Vec::new();

    for root_no in &root_candidates {
        if let Some(root_node) = build_subtree(root_no, &index, &children_map, 0, root_no) {
            roots.push(root_node);
        }
    }

    roots
}

fn build_subtree(
    material_no: &str,
    index: &HashMap<String, BomTreeNode>,
    children_map: &HashMap<String, Vec<String>>,
    level: usize,
    path: &str,
) -> Option<BomTreeNode> {
    let mut node = index.get(material_no)?.clone();
    node.level = level;
    node.path = path.to_string();

    if let Some(child_nos) = children_map.get(material_no) {
        for child_no in child_nos {
            let child_path = format!("{}/{}", path, child_no);
            if let Some(child) = build_subtree(child_no, index, children_map, level + 1, &child_path) {
                node.children.push(child);
            }
        }
    }

    Some(node)
}

pub fn flatten(tree: &[BomTreeNode]) -> BomData {
    let mut result = Vec::new();
    for node in tree {
        flatten_node(node, None, &mut result);
    }
    result
}

fn flatten_node(node: &BomTreeNode, parent: Option<&str>, result: &mut BomData) {
    let item = BomItem {
        material_no: node.material_no.clone(),
        material_name: node.material_name.clone(),
        specification: node.specification.clone(),
        bom_type: node.bom_type,
        quantity: node.quantity,
        unit: node.unit.clone(),
        loss_rate: node.loss_rate,
        supplier: node.supplier.clone(),
        unit_price: node.unit_price,
        source: node.source,
        effective_date: node.effective_date.clone(),
        expiry_date: node.expiry_date.clone(),
        remark: node.remark.clone(),
        parent_material_no: parent.map(|s| s.to_string()),
    };
    result.push(item);

    for child in &node.children {
        flatten_node(child, Some(&node.material_no), result);
    }
}

pub trait TreeVisitor {
    fn enter(&mut self, node: &BomTreeNode) -> bool;
    fn leave(&mut self, _node: &BomTreeNode) {}
}

pub fn traverse_pre_order<F: FnMut(&BomTreeNode)>(tree: &[BomTreeNode], mut f: F) {
    struct PreOrderVisitor<F: FnMut(&BomTreeNode)> {
        f: F,
    }
    impl<F: FnMut(&BomTreeNode)> TreeVisitor for PreOrderVisitor<F> {
        fn enter(&mut self, node: &BomTreeNode) -> bool {
            (self.f)(node);
            true
        }
    }
    traverse(tree, &mut PreOrderVisitor { f });
}

pub fn traverse_post_order<F: FnMut(&BomTreeNode)>(tree: &[BomTreeNode], mut f: F) {
    struct PostOrderVisitor<F: FnMut(&BomTreeNode)> {
        f: F,
    }
    impl<F: FnMut(&BomTreeNode)> TreeVisitor for PostOrderVisitor<F> {
        fn enter(&mut self, _node: &BomTreeNode) -> bool {
            true
        }
        fn leave(&mut self, node: &BomTreeNode) {
            (self.f)(node);
        }
    }
    traverse(tree, &mut PostOrderVisitor { f });
}

pub fn traverse<V: TreeVisitor>(tree: &[BomTreeNode], visitor: &mut V) {
    for node in tree {
        traverse_node(node, visitor);
    }
}

fn traverse_node<V: TreeVisitor>(node: &BomTreeNode, visitor: &mut V) {
    let continue_traverse = visitor.enter(node);
    if continue_traverse {
        for child in &node.children {
            traverse_node(child, visitor);
        }
    }
    visitor.leave(node);
}

pub fn collect_raw_materials(tree: &[BomTreeNode]) -> Vec<BomTreeNode> {
    let mut materials = Vec::new();
    traverse_pre_order(tree, |node| {
        if node.is_leaf() && !node.is_virtual() {
            materials.push(node.clone());
        }
    });
    materials
}

pub fn max_depth(tree: &[BomTreeNode]) -> usize {
    let mut max = 0;
    traverse_pre_order(tree, |node| {
        if node.level + 1 > max {
            max = node.level + 1;
        }
    });
    max
}

pub fn find_node(tree: &[BomTreeNode], material_no: &str) -> Option<BomTreeNode> {
    let mut result = None;
    traverse_pre_order(tree, |node| {
        if node.material_no == material_no {
            result = Some(node.clone());
        }
    });
    result
}

pub fn count_nodes(tree: &[BomTreeNode]) -> usize {
    let mut count = 0;
    traverse_pre_order(tree, |_| count += 1);
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bom::{SourceType, BomType};

    fn sample_bom_data() -> BomData {
        vec![
            BomItem {
                material_no: "B001".to_string(),
                material_name: "半成品B".to_string(),
                specification: "".to_string(),
                bom_type: BomType::SemiFinished,
                quantity: 1.0,
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
                material_no: "C001".to_string(),
                material_name: "零件C".to_string(),
                specification: "".to_string(),
                bom_type: BomType::RawMaterial,
                quantity: 3.0,
                unit: "个".to_string(),
                loss_rate: 0.02,
                supplier: "供应商X".to_string(),
                unit_price: 10.0,
                source: SourceType::Purchased,
                effective_date: "".to_string(),
                expiry_date: "".to_string(),
                remark: "".to_string(),
                parent_material_no: Some("B001".to_string()),
            },
        ]
    }

    #[test]
    fn test_build_tree_out_of_order() {
        let items = sample_bom_data();
        let tree = build_tree(&items);

        assert_eq!(tree.len(), 1);
        let root = &tree[0];
        assert_eq!(root.material_no, "A001");
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].material_no, "B001");
        assert_eq!(root.children[0].children.len(), 1);
        assert_eq!(root.children[0].children[0].material_no, "C001");
    }

    #[test]
    fn test_max_depth() {
        let items = sample_bom_data();
        let tree = build_tree(&items);
        assert_eq!(max_depth(&tree), 3);
    }

    #[test]
    fn test_find_node() {
        let items = sample_bom_data();
        let tree = build_tree(&items);
        let found = find_node(&tree, "B001");
        assert!(found.is_some());
        assert_eq!(found.unwrap().material_no, "B001");
    }

    #[test]
    fn test_flatten_roundtrip() {
        let items = sample_bom_data();
        let tree = build_tree(&items);
        let flat = flatten(&tree);
        assert_eq!(flat.len(), 3);
    }

    #[test]
    fn test_virtual_node() {
        let mut items = sample_bom_data();
        items.push(BomItem {
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
            parent_material_no: Some("B001".to_string()),
        });
        let tree = build_tree(&items);
        let v_node = find_node(&tree, "V001").unwrap();
        assert!(v_node.is_virtual());
    }
}
