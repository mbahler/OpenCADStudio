// Model module — 3D solid modelling.
//
//   Model group  : create primitive solids (box, cylinder, cone, sphere, …)
//                  as ACIS Solid3D entities via acadrust's primitive builders.
//   Design group : combine solids with truck boolean operations
//                  (union / subtract / intersect).

pub mod boolean_cmd;
pub mod primitive_cmd;

use crate::modules::{CadModule, IconKind, ModuleEvent, RibbonGroup, RibbonItem, ToolDef};

pub struct ModelModule;

/// Helper to declare a ribbon tool that fires a named command.
fn tool(id: &'static str, label: &'static str, glyph: &'static str) -> ToolDef {
    ToolDef {
        id,
        label,
        icon: IconKind::Glyph(glyph),
        event: ModuleEvent::Command(id.to_string()),
    }
}

impl CadModule for ModelModule {
    fn id(&self) -> &'static str {
        "model"
    }
    fn title(&self) -> &'static str {
        "Model"
    }

    fn ribbon_groups(&self) -> Vec<RibbonGroup> {
        vec![
            RibbonGroup {
                title: "Model",
                tools: vec![
                    RibbonItem::LargeTool(tool("BOX", "Box", "▦")),
                    RibbonItem::LargeTool(tool("CYLINDER", "Cylinder", "⬭")),
                    RibbonItem::LargeTool(tool("CONE", "Cone", "△")),
                    RibbonItem::LargeTool(tool("SPHERE", "Sphere", "◯")),
                    RibbonItem::Dropdown {
                        id: "MODEL_MORE",
                        icon: IconKind::Glyph("◰"),
                        items: vec![
                            ("WEDGE", "Wedge", IconKind::Glyph("◣")),
                            ("TORUS", "Torus", IconKind::Glyph("◎")),
                        ],
                        default: "WEDGE",
                    },
                ],
            },
            RibbonGroup {
                title: "Design",
                tools: vec![
                    RibbonItem::LargeTool(tool("UNION", "Union", "⊕")),
                    RibbonItem::LargeTool(tool("SUBTRACT", "Subtract", "⊖")),
                    RibbonItem::LargeTool(tool("INTERSECT", "Intersect", "⊗")),
                ],
            },
        ]
    }
}
