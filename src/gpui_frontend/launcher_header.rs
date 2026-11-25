use std::sync::{Arc, OnceLock};

use gpui_kit::component::label::Label;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    FontWeight, Image, ImageFormat, InteractiveElement, IntoElement, ParentElement, Role, StatefulInteractiveElement, Styled,
    div, img, px,
};
use multi_launcher::gui::theme::ThemeMetrics;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Branding {
    pub icon: bool,
    pub title: bool,
}

impl Branding {
    fn visible(self) -> bool {
        self.icon || self.title
    }
}

fn badge_size(heading_height: f32) -> f32 {
    (heading_height * 1.55).clamp(26.0, 34.0)
}

pub(super) fn identity(branding: Branding, metrics: ThemeMetrics) -> Option<impl IntoElement> {
    if !branding.visible() {
        return None;
    }
    static BADGE: OnceLock<Arc<Image>> = OnceLock::new();
    let heading_line_height = metrics.heading_text_size * 1.15;
    let size = badge_size(heading_line_height);
    Some(
        div()
            .id("launcher-identity")
            .flex()
            .items_center()
            .gap_x(px(metrics.item_spacing_x))
            .when(branding.icon, |row| {
                row.child(
                    img(Arc::clone(BADGE.get_or_init(|| {
                        Arc::new(Image::from_bytes(
                            ImageFormat::Png,
                            include_bytes!("../../Resources/Green_MultiLauncher.png").to_vec(),
                        ))
                    })))
                    .size(px(size)),
                )
            })
            .when(branding.title, |row| {
                row.child(
                    div().id("launcher-identity-heading").role(Role::Heading).child(
                        Label::new("Multi Launcher")
                            .id("launcher-identity-title")
                            .text_size(px(metrics.heading_text_size))
                            .line_height(px(heading_line_height))
                            .font_weight(FontWeight::BOLD),
                    ),
                )
            }),
    )
}

#[cfg(test)]
mod tests {
    use super::{Branding, badge_size};

    #[test]
    fn branding_controls_and_badge_size_are_independent() {
        assert!(!Branding::default().visible());
        assert!(Branding { icon: true, title: false }.visible());
        assert!(Branding { icon: false, title: true }.visible());
        assert_eq!(badge_size(10.0), 26.0);
        assert!((badge_size(18.0 * 1.15) - 32.085).abs() < 0.001);
        assert_eq!(badge_size(30.0), 34.0);
    }
}
