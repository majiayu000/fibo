use super::{Scheme, Theme, mix};

#[test]
fn mix_blends_each_channel() {
    assert_eq!(mix(0x000000, 0xffffff, 0), 0x000000);
    assert_eq!(mix(0x000000, 0xffffff, 1000), 0xffffff);
    assert_eq!(mix(0x102030, 0x304050, 500), 0x203040);
    assert_eq!(Theme::pane(), Theme::terminal_bg());
}

#[test]
fn lifted_schemes_keep_the_dark_sidebar_ladder_on_their_own_colors() {
    use crate::ui::palette::{Palette, restate};
    for scheme in Scheme::ALL {
        assert_eq!(Palette::of_scheme(scheme).terminal_bg, scheme.bg());
        if scheme.is_light() || scheme == Scheme::Dark {
            continue;
        }
        let restate = |dark| restate(dark, scheme.bg(), scheme.fg());
        assert_eq!(restate(Scheme::Dark.bg()), scheme.bg());
        assert_eq!(restate(Scheme::Dark.fg()), scheme.fg());
        // The sidebar's grays keep their order from ground to bright text.
        let ladder = [
            0x0c0c0f, 0x121216, 0x1c1c21, 0x45454d, 0x7e7e87, 0xc9c9cf, 0xececee,
        ];
        for pair in ladder.windows(2) {
            assert!(
                crate::ui::palette::gray(restate(pair[0]))
                    < crate::ui::palette::gray(restate(pair[1]))
            );
        }
        // Text stays readable on the ground: about 10:1, never below 7:1.
        let lum = |c: u32| {
            let ch = |v: u32| {
                let v = v as f64 / 255.;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * ch(c >> 16 & 255) + 0.7152 * ch(c >> 8 & 255) + 0.0722 * ch(c & 255)
        };
        let ratio = (lum(scheme.fg()) + 0.05) / (lum(scheme.bg()) + 0.05);
        assert!(
            (7. ..13.).contains(&ratio),
            "{scheme:?} contrast {ratio:.1}"
        );
    }
}

#[test]
fn light_scheme_swaps_every_surface() {
    use super::{DARK, Palette};
    const LIGHT: Palette = Palette::of_scheme(Scheme::Light);
    assert_eq!(LIGHT.terminal_bg, Scheme::Light.bg());
    assert!(LIGHT.app < LIGHT.pane && DARK.app < DARK.pane);
    // Text stays far from its background in both schemes.
    for p in [DARK, LIGHT] {
        let gap = |a: u32, b: u32| ((a & 255) as i32 - (b & 255) as i32).abs();
        assert!(gap(p.body, p.pane) > 180);
        assert!(gap(p.secondary, p.pane) > 100);
        assert!(gap(p.on_accent, p.accent) > 180);
    }
}

/// Light meets the contrast floors: text 4.5:1, focus and field edges 3:1, status
/// dots 3:1 even for the pale yellow and green hues; its surfaces step visibly.
#[test]
fn light_scheme_meets_contrast_floors() {
    use super::{Palette, Status, contrast, status_colors_in};
    let p = Palette::of_scheme(Scheme::Light);
    for (name, text) in [
        ("body", p.body),
        ("secondary", p.secondary),
        ("muted", p.muted),
    ] {
        assert!(contrast(text, p.pane) >= 4.5, "{name}");
    }
    assert!(contrast(p.focus, p.pane) >= 3.);
    assert!(contrast(p.field_edge, p.pane) >= 3.);
    assert!(
        contrast(p.focus, p.field_edge) >= 1.5,
        "focus must differ from the edge"
    );
    assert!(contrast(p.app, p.pane) >= 1.15);
    assert!(contrast(p.card_border, p.pane) >= 1.6);
    // The sidebar is the one gray step under the white panes (the v3 design).
    assert!(contrast(p.pane, p.sidebar) >= 1.05);
    assert!(
        contrast(p.raised, p.sidebar) >= 1.05,
        "selected row on the sidebar"
    );
    for group in Status::ALL {
        let (dot, ink, _) = status_colors_in(group, &p);
        assert!(contrast(dot, p.pane) >= 3., "{} dot", group.label());
        assert!(contrast(ink, p.pane) >= 4.5, "{} ink", group.label());
    }
}

/// Colors and sizes come from `Theme`; a literal in a component drifts from the tokens.
/// The terminal renderer and the visual-test harness measure cells, not chrome.
#[test]
fn components_take_colors_and_sizes_from_tokens() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<_> = [
        "terminal/pane.rs",
        "ui/field.rs",
        "ui/widgets.rs",
        "desktop.rs",
    ]
    .iter()
    .map(|name| src.join(name))
    .collect();
    // Every file under workbench/, its submodule folders included.
    let mut dirs = vec![src.join("workbench")];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                files.push(path);
            }
        }
    }
    let mut offenders = Vec::new();
    for path in files {
        let text = std::fs::read_to_string(&path).unwrap();
        for (n, line) in text.lines().enumerate() {
            if color_literal(line) || size_literal(line) {
                offenders.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "use Theme tokens:\n{}",
        offenders.join("\n")
    );
}

fn color_literal(line: &str) -> bool {
    line.match_indices("0x").any(|(at, _)| {
        let hex = line[at + 2..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .count();
        hex == 6
    })
}

fn size_literal(line: &str) -> bool {
    line.match_indices("px(").any(|(at, _)| {
        let rest = &line[at + 3..];
        rest.starts_with(|c: char| c.is_ascii_digit()) && !rest.starts_with("0.)")
    })
}
