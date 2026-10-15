//! La page de présentation.

use super::Sheet;

pub fn site() -> String {
    let mut s = Sheet::default();
    // Alley — la vitrine.
    s.rule(
        ":root",
        &[
            ("--bg", "#0f0d16"),
            ("--bg-2", "#16131f"),
            ("--card", "#1c1828"),
            ("--line", "#2f2a44"),
            ("--fg", "#f1eefb"),
            ("--muted", "#a79fc4"),
            ("--accent", "#7c5cff"),
            ("--accent-2", "#22d3a6"),
            ("--radius", "18px"),
            ("--font", r#""Inter", system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif"#),
            ("--mono", r#"ui-monospace, "SF Mono", "Cascadia Code", Menlo, Consolas, monospace"#),
            ("color-scheme", "dark"),
        ],
    );
    s.media("(prefers-color-scheme: light)", |s| {
        s.rule(
            ":root",
            &[
                ("--bg", "#faf9fe"),
                ("--bg-2", "#f1eefa"),
                ("--card", "#ffffff"),
                ("--line", "#e1dcf0"),
                ("--fg", "#1b1630"),
                ("--muted", "#5f5880"),
                ("color-scheme", "light"),
            ],
        );
    });
    s.rule("*", &[("box-sizing", "border-box")]);
    s.rule(
        "html",
        &[
            ("scroll-behavior", "smooth"),
            ("-webkit-text-size-adjust", "100%"),
        ],
    );
    s.rule(
        "body",
        &[
            ("overflow-x", "hidden"),
            ("margin", "0"),
            ("background", "var(--bg)"),
            ("color", "var(--fg)"),
            ("font", "17px/1.6 var(--font)"),
            ("padding", "0 env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left)"),
        ],
    );
    s.rule("a", &[("color", "inherit")]);
    s.rule(
        "h1, h2, h3",
        &[("line-height", "1.15"), ("letter-spacing", "-0.02em")],
    );
    s.rule("img", &[("max-width", "100%")]);
    s.rule(
        ".button",
        &[
            ("display", "inline-flex"),
            ("align-items", "center"),
            ("justify-content", "center"),
            ("min-height", "48px"),
            ("padding", "0 22px"),
            ("border-radius", "14px"),
            (
                "background",
                "linear-gradient(135deg, var(--accent), #5b8cff 60%, var(--accent-2))",
            ),
            ("color", "#fff"),
            ("font-weight", "700"),
            ("text-decoration", "none"),
            ("border", "0"),
            ("cursor", "pointer"),
            ("font", "inherit"),
            ("font-weight", "700"),
            ("box-shadow", "0 10px 30px -10px var(--accent)"),
        ],
    );
    s.rule(
        ".button.small",
        &[
            ("min-height", "38px"),
            ("padding", "0 16px"),
            ("font-size", "15px"),
            ("box-shadow", "none"),
        ],
    );
    s.rule(
        ".button.ghost",
        &[
            ("background", "transparent"),
            ("color", "var(--fg)"),
            ("border", "1px solid var(--line)"),
            ("box-shadow", "none"),
        ],
    );
    s.rule(".button:hover", &[("filter", "brightness(1.08)")]);
    s.rule(
        ".top",
        &[
            ("position", "sticky"),
            ("top", "0"),
            ("z-index", "10"),
            ("display", "flex"),
            ("align-items", "center"),
            ("justify-content", "space-between"),
            ("gap", "16px"),
            (
                "padding",
                "calc(12px + env(safe-area-inset-top)) clamp(16px, 4vw, 48px) 12px",
            ),
            (
                "background",
                "color-mix(in srgb, var(--bg) 80%, transparent)",
            ),
            ("backdrop-filter", "blur(12px)"),
            ("-webkit-backdrop-filter", "blur(12px)"),
            ("border-bottom", "1px solid var(--line)"),
        ],
    );
    s.rule(
        ".logo",
        &[
            ("display", "inline-flex"),
            ("align-items", "center"),
            ("gap", "10px"),
            ("font-weight", "800"),
            ("font-size", "20px"),
            ("text-decoration", "none"),
        ],
    );
    s.rule(
        ".top nav",
        &[
            ("display", "flex"),
            ("align-items", "center"),
            ("gap", "clamp(10px, 2vw, 28px)"),
        ],
    );
    s.rule(
        ".top nav a:not(.button)",
        &[
            ("color", "var(--muted)"),
            ("text-decoration", "none"),
            ("font-size", "15px"),
        ],
    );
    s.rule(".top nav a:not(.button):hover", &[("color", "var(--fg)")]);
    s.rule(
        ".hero",
        &[
            ("display", "grid"),
            ("grid-template-columns", "minmax(0, 1.05fr) minmax(0, 1fr)"),
            ("gap", "clamp(24px, 5vw, 64px)"),
            ("align-items", "center"),
            ("max-width", "1200px"),
            ("margin", "0 auto"),
            ("padding", "clamp(40px, 8vw, 110px) clamp(16px, 4vw, 48px)"),
            ("position", "relative"),
        ],
    );
    s.rule(
        ".hero::before",
        &[
            ("content", r#""""#),
            ("position", "absolute"),
            ("top", "0"),
            ("bottom", "0"),
            ("left", "50%"),
            ("width", "100vw"),
            ("transform", "translateX(-50%)"),
            ("z-index", "-1"),
            ("pointer-events", "none"),
            ("background", "radial-gradient(600px 300px at 15% 20%, color-mix(in srgb, var(--accent) 22%, transparent), transparent), radial-gradient(500px 300px at 85% 70%, color-mix(in srgb, var(--accent-2) 16%, transparent), transparent)"),
        ],
    );
    s.rule(
        ".eyebrow",
        &[
            ("color", "var(--accent-2)"),
            ("font-weight", "700"),
            ("font-size", "14px"),
            ("margin", "0"),
        ],
    );
    s.rule(
        ".hero h1",
        &[
            ("font-size", "clamp(36px, 5vw, 58px)"),
            ("margin", "12px 0 18px"),
        ],
    );
    s.rule(
        ".lead",
        &[
            ("color", "var(--muted)"),
            ("font-size", "clamp(17px, 1.8vw, 20px)"),
            ("margin", "0"),
            ("max-width", "60ch"),
        ],
    );
    s.rule(
        ".cta",
        &[
            ("display", "flex"),
            ("flex-wrap", "wrap"),
            ("gap", "12px"),
            ("margin", "28px 0 14px"),
        ],
    );
    s.rule(
        ".hint",
        &[
            ("color", "var(--muted)"),
            ("font-size", "14px"),
            ("margin", "0"),
        ],
    );
    s.rule(".demo", &[("display", "grid"), ("gap", "14px")]);
    s.rule(
        ".pane",
        &[
            ("background", "var(--card)"),
            ("border", "1px solid var(--line)"),
            ("border-radius", "var(--radius)"),
            ("padding", "16px"),
            ("display", "grid"),
            ("gap", "12px"),
            ("box-shadow", "0 30px 60px -30px #000a"),
        ],
    );
    s.rule(
        ".pane.server",
        &[
            ("background", "var(--bg-2)"),
            ("transform", "translateX(clamp(0px, 3vw, 32px))"),
        ],
    );
    s.rule(
        ".pane-title",
        &[
            ("font-size", "13px"),
            ("color", "var(--muted)"),
            ("font-weight", "700"),
            ("text-transform", "uppercase"),
            ("letter-spacing", "0.05em"),
        ],
    );
    s.rule(
        ".msg",
        &[
            ("display", "flex"),
            ("gap", "12px"),
            ("align-items", "flex-start"),
        ],
    );
    s.rule(".msg p", &[("margin", "2px 0 0")]);
    s.rule(".msg strong", &[("font-size", "15px")]);
    s.rule(
        ".avatar",
        &[
            ("flex", "none"),
            ("width", "36px"),
            ("height", "36px"),
            ("border-radius", "50%"),
            ("display", "grid"),
            ("place-items", "center"),
            ("background", "var(--c)"),
            ("color", "#fff"),
            ("font-weight", "800"),
        ],
    );
    s.rule(
        ".msg.cipher code",
        &[
            ("font-family", "var(--mono)"),
            ("font-size", "13px"),
            ("color", "var(--accent-2)"),
            ("overflow-wrap", "anywhere"),
        ],
    );
    s.rule(
        ".section",
        &[
            ("max-width", "1200px"),
            ("margin", "0 auto"),
            ("padding", "clamp(48px, 7vw, 96px) clamp(16px, 4vw, 48px)"),
        ],
    );
    s.rule(".section.narrow", &[("max-width", "820px")]);
    s.rule(
        ".section h2",
        &[
            ("font-size", "clamp(30px, 4vw, 44px)"),
            ("margin", "0 0 12px"),
            ("text-align", "center"),
        ],
    );
    s.rule(
        ".section-lead",
        &[
            ("color", "var(--muted)"),
            ("text-align", "center"),
            ("margin", "0 auto 36px"),
            ("max-width", "60ch"),
        ],
    );
    s.rule(
        ".grid",
        &[
            ("display", "grid"),
            (
                "grid-template-columns",
                "repeat(auto-fit, minmax(min(100%, 280px), 1fr))",
            ),
            ("gap", "16px"),
        ],
    );
    s.rule(
        ".card, .platform",
        &[
            ("background", "var(--card)"),
            ("border", "1px solid var(--line)"),
            ("border-radius", "var(--radius)"),
            ("padding", "22px"),
        ],
    );
    s.rule(
        ".card h3, .platform h3",
        &[("margin", "10px 0 6px"), ("font-size", "19px")],
    );
    s.rule(
        ".card p, .platform p",
        &[
            ("margin", "0"),
            ("color", "var(--muted)"),
            ("font-size", "15px"),
        ],
    );
    s.rule(".icon", &[("font-size", "28px")]);
    s.rule(
        ".steps",
        &[
            ("list-style", "none"),
            ("padding", "0"),
            ("margin", "36px auto"),
            ("max-width", "860px"),
            ("display", "grid"),
            ("gap", "14px"),
        ],
    );
    s.rule(
        ".step",
        &[
            ("display", "flex"),
            ("gap", "16px"),
            ("align-items", "flex-start"),
            ("background", "var(--card)"),
            ("border", "1px solid var(--line)"),
            ("border-radius", "var(--radius)"),
            ("padding", "18px 20px"),
        ],
    );
    s.rule(".step h3", &[("margin", "0 0 4px"), ("font-size", "18px")]);
    s.rule(".step p", &[("margin", "0"), ("color", "var(--muted)")]);
    s.rule(
        ".num",
        &[
            ("flex", "none"),
            ("display", "grid"),
            ("place-items", "center"),
            ("width", "40px"),
            ("height", "40px"),
            ("border-radius", "12px"),
            (
                "background",
                "linear-gradient(135deg, var(--accent), var(--accent-2))",
            ),
            ("color", "#fff"),
            ("font-weight", "800"),
        ],
    );
    s.rule(".tech", &[("margin-top", "24px")]);
    s.rule(
        ".install-now",
        &[
            ("display", "grid"),
            ("justify-items", "center"),
            ("gap", "8px"),
            ("margin-bottom", "28px"),
        ],
    );
    s.rule(
        ".platform.current",
        &[
            ("border-color", "var(--accent)"),
            (
                "box-shadow",
                "0 0 0 1px var(--accent), 0 20px 40px -20px var(--accent)",
            ),
        ],
    );
    s.rule(
        ".platform.current h3::after",
        &[
            ("content", r#"" · votre appareil""#),
            ("color", "var(--accent-2)"),
            ("font-size", "14px"),
            ("font-weight", "600"),
        ],
    );
    s.rule(
        "details",
        &[
            ("border-bottom", "1px solid var(--line)"),
            ("padding", "14px 0"),
        ],
    );
    s.rule(
        "summary",
        &[
            ("cursor", "pointer"),
            ("font-weight", "700"),
            ("font-size", "18px"),
            ("list-style", "none"),
            ("display", "flex"),
            ("justify-content", "space-between"),
            ("gap", "12px"),
        ],
    );
    s.rule("summary::-webkit-details-marker", &[("display", "none")]);
    s.rule(
        "summary::after",
        &[
            ("content", r#""+""#),
            ("color", "var(--accent-2)"),
            ("font-size", "22px"),
            ("line-height", "1"),
        ],
    );
    s.rule("details[open] summary::after", &[("content", r#""–""#)]);
    s.rule(
        "details p",
        &[("color", "var(--muted)"), ("margin", "10px 0 0")],
    );
    s.rule(
        ".bottom",
        &[
            ("display", "flex"),
            ("flex-wrap", "wrap"),
            ("align-items", "center"),
            ("justify-content", "space-between"),
            ("gap", "16px"),
            (
                "padding",
                "28px clamp(16px, 4vw, 48px) calc(28px + env(safe-area-inset-bottom))",
            ),
            ("border-top", "1px solid var(--line)"),
            ("color", "var(--muted)"),
        ],
    );
    s.rule(".bottom p", &[("margin", "0")]);
    // Le pied de page : la marque, puis trois colonnes de liens.
    s.rule(
        ".bottom",
        &[
            ("display", "grid"),
            ("gap", "28px"),
            ("justify-content", "stretch"),
            ("background", "var(--bg-2)"),
        ],
    );
    s.rule(
        ".footer-grid",
        &[
            ("display", "grid"),
            (
                "grid-template-columns",
                "minmax(220px, 1.4fr) repeat(3, minmax(140px, 1fr))",
            ),
            ("gap", "32px"),
            ("max-width", "1180px"),
            ("width", "100%"),
            ("margin", "0 auto"),
        ],
    );
    s.rule(
        ".footer-brand",
        &[
            ("display", "grid"),
            ("gap", "14px"),
            ("align-content", "start"),
            ("justify-items", "start"),
        ],
    );
    s.rule(
        ".footer-brand p",
        &[("max-width", "32ch"), ("font-size", "15px")],
    );
    s.rule(
        ".bottom nav h2",
        &[
            ("margin", "0 0 10px"),
            ("font-size", "13px"),
            ("letter-spacing", ".08em"),
            ("text-transform", "uppercase"),
            ("color", "var(--fg)"),
        ],
    );
    s.rule(
        ".bottom nav ul",
        &[
            ("list-style", "none"),
            ("margin", "0"),
            ("padding", "0"),
            ("display", "grid"),
            ("gap", "8px"),
        ],
    );
    s.rule(
        ".bottom nav ul a",
        &[
            ("color", "var(--muted)"),
            ("text-decoration", "none"),
            ("font-size", "15px"),
        ],
    );
    s.rule(
        ".bottom nav ul a:hover, .bottom nav ul a:focus-visible",
        &[("color", "var(--fg)"), ("text-decoration", "underline")],
    );
    s.rule(
        ".footer-bar",
        &[
            ("display", "flex"),
            ("flex-wrap", "wrap"),
            ("align-items", "center"),
            ("justify-content", "space-between"),
            ("gap", "12px"),
            ("max-width", "1180px"),
            ("width", "100%"),
            ("margin", "0 auto"),
            ("padding-top", "20px"),
            ("border-top", "1px solid var(--line)"),
            ("font-size", "14px"),
        ],
    );
    // Les tarifs.
    s.rule(
        ".prices",
        &[(
            "grid-template-columns",
            "repeat(auto-fit, minmax(240px, 1fr))",
        )],
    );
    s.rule(
        ".price-card .amount",
        &[
            ("font-size", "30px"),
            ("font-weight", "800"),
            ("margin", "4px 0"),
            ("color", "var(--fg)"),
        ],
    );
    s.rule(
        ".price-card .amount span",
        &[
            ("font-size", "15px"),
            ("font-weight", "500"),
            ("color", "var(--muted)"),
        ],
    );
    s.rule(
        ".hint.center",
        &[("text-align", "center"), ("margin-top", "18px")],
    );
    // Les pages légales.
    s.rule(
        ".legal",
        &[
            ("display", "grid"),
            ("gap", "6px"),
            ("padding-bottom", "24px"),
        ],
    );
    s.rule(
        ".legal h1",
        &[
            ("font-size", "clamp(30px, 5vw, 44px)"),
            ("margin", "8px 0 0"),
        ],
    );
    s.rule(
        ".legal h2",
        &[("font-size", "21px"), ("margin", "26px 0 4px")],
    );
    s.rule(
        ".legal p",
        &[("margin", "6px 0"), ("color", "var(--muted)")],
    );
    s.rule(
        ".legal .lead",
        &[("font-size", "19px"), ("color", "var(--fg)")],
    );
    s.rule(".legal .updated", &[("font-size", "14px")]);
    s.rule(".legal .button", &[("margin-top", "18px")]);
    // Les trois langues.
    s.rule(
        "nav.langs",
        &[("display", "flex"), ("gap", "4px"), ("font-size", "13px")],
    );
    s.rule(
        "nav.langs a",
        &[
            ("display", "inline-block"),
            ("padding", "4px 8px"),
            ("border-radius", "8px"),
            ("color", "var(--muted)"),
            ("text-decoration", "none"),
            ("font-weight", "700"),
        ],
    );
    s.rule(
        "nav.langs a[aria-current]",
        &[("background", "var(--card)"), ("color", "var(--fg)")],
    );
    s.rule(
        ".notfound > div",
        &[
            ("display", "grid"),
            ("gap", "10px"),
            ("justify-items", "center"),
        ],
    );
    s.rule(
        ".notfound",
        &[
            ("grid-template-columns", "1fr"),
            ("text-align", "center"),
            ("justify-items", "center"),
        ],
    );
    s.media("(max-width: 860px)", |s| {
        s.rule(".footer-grid", &[("grid-template-columns", "1fr 1fr")]);
        s.rule(".footer-brand", &[("grid-column", "1 / -1")]);
        s.rule(".hero", &[("grid-template-columns", "1fr")]);
        s.rule(".pane.server", &[("transform", "none")]);
        s.rule(".top nav a:not(.button)", &[("display", "none")]);
        s.rule(
            ".top nav.langs a:not(.button)",
            &[("display", "inline-block")],
        );
        s.rule(".top", &[("flex-wrap", "wrap")]);
    });
    s.css()
}
