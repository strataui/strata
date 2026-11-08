//! Declarative 3D Canvas Viewport extraction and DrawViewport3D command generation.

use super::*;

impl AppRuntime {
    /// Extracts declarative 3D canvas viewports from the blueprint and layout hierarchies and emits DrawViewport3D commands.
    pub fn render_canvas_3d_scenes(&mut self) {
        Self::collect_canvas_3d_recursive(
            &self.root_blueprint,
            &self.layout_root,
            &mut self.display_list.commands,
        );
    }

    fn collect_canvas_3d_recursive(
        blueprint: &NodeBlueprint,
        layout_node: &LayoutNode,
        commands: &mut Vec<strata_render::compositor::DrawCommand>,
    ) {
        match blueprint {
            NodeBlueprint::Element {
                tag,
                attributes,
                children,
                ..
            } => {
                let is_3d_canvas = matches!(tag, Tag::Canvas | Tag::Custom("Canvas3D"))
                    && (attributes.iter().any(|a| {
                        matches!(a, strata_dom::Attribute::Property { key, value } if key == "data-strata-canvas3d" && value == "true")
                    }) || children.iter().any(|c| match c {
                        NodeBlueprint::Element {
                            tag: Tag::Custom(name),
                            ..
                        } => {
                            matches!(
                                *name,
                                "PerspectiveCamera"
                                    | "OrthographicCamera"
                                    | "AmbientLight"
                                    | "DirectionalLight"
                                    | "PointLight"
                                    | "Mesh"
                                    | "BoxGeometry"
                                    | "SphereGeometry"
                                    | "PlaneGeometry"
                                    | "CylinderGeometry"
                                    | "StandardMaterial"
                                    | "ShaderMaterial"
                            )
                        }
                        _ => false,
                    }));

                if is_3d_canvas {
                    let scene = strata_render::scene3d::compile_scene3d_from_blueprint(blueprint);
                    let bounds = if layout_node.geometry.content_box.width > 0.0
                        && layout_node.geometry.content_box.height > 0.0
                    {
                        layout_node.geometry.content_box
                    } else {
                        layout_node.geometry.border_box
                    };
                    commands.push(strata_render::compositor::DrawCommand::DrawViewport3D {
                        bounds,
                        scene,
                    });
                    return;
                }

                let mut flat_children = Vec::new();
                for c in children {
                    Self::flatten_blueprints(c, &mut flat_children);
                }

                for (child_bp, child_layout) in flat_children.iter().zip(layout_node.children.iter()) {
                    Self::collect_canvas_3d_recursive(
                        child_bp,
                        child_layout,
                        commands,
                    );
                }
            }
            NodeBlueprint::Fragment(children) => {
                let mut flat_children = Vec::new();
                for c in children {
                    Self::flatten_blueprints(c, &mut flat_children);
                }
                for (child_bp, child_layout) in flat_children.iter().zip(layout_node.children.iter()) {
                    Self::collect_canvas_3d_recursive(
                        child_bp,
                        child_layout,
                        commands,
                    );
                }
            }
            _ => {}
        }
    }
}
