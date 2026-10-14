//! L'application : le hall, les salons, les appels.

use super::Sheet;

pub fn app() -> String {
    let mut s = Sheet::default();
    // Alley — l'application. Sombre par défaut, clair si le système le demande.
    // Pensée pour le doigt (iPhone, Android) comme pour la souris.
    s.rule(
        ":root",
        &[
            ("--bg", "#16131f"),
            ("--rail", "#0f0d16"),
            ("--side", "#1c1828"),
            ("--chat", "#221e30"),
            ("--elev", "#2b2640"),
            ("--hover", "#2f2945"),
            ("--line", "#342f4a"),
            ("--fg", "#ece9f7"),
            ("--muted", "#a39cbf"),
            ("--faint", "#6f6890"),
            ("--accent", "#7c5cff"),
            ("--accent-2", "#22d3a6"),
            ("--accent-fg", "#fff"),
            ("--danger", "#ff5d73"),
            ("--radius", "12px"),
            ("--font", r#""Inter", system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, "Noto Sans", sans-serif"#),
            ("--mono", r#"ui-monospace, "SF Mono", "Cascadia Code", Menlo, Consolas, monospace"#),
            ("--app-height", "100dvh"),
            ("--safe-top", "env(safe-area-inset-top, 0px)"),
            ("--safe-bottom", "env(safe-area-inset-bottom, 0px)"),
            ("--safe-left", "env(safe-area-inset-left, 0px)"),
            ("--safe-right", "env(safe-area-inset-right, 0px)"),
            ("color-scheme", "dark"),
        ],
    );
    s.media("(prefers-color-scheme: light)", |s| {
        s.rule(
            ":root",
            &[
                ("--bg", "#f4f2fb"),
                ("--rail", "#e6e2f3"),
                ("--side", "#efecf8"),
                ("--chat", "#ffffff"),
                ("--elev", "#ffffff"),
                ("--hover", "#ece8f8"),
                ("--line", "#dcd6ee"),
                ("--fg", "#1b1630"),
                ("--muted", "#625a80"),
                ("--faint", "#9a93b5"),
                ("color-scheme", "light"),
            ],
        );
    });
    s.rule("*", &[("box-sizing", "border-box")]);
    s.rule(
        "html, body",
        &[
            ("margin", "0"),
            ("height", "100%"),
            ("background", "var(--bg)"),
            ("color", "var(--fg)"),
            ("font", "16px/1.45 var(--font)"),
            ("-webkit-text-size-adjust", "100%"),
        ],
    );
    s.rule(
        "body",
        &[("overflow", "hidden"), ("overscroll-behavior", "none")],
    );
    s.rule(
        "button, input, textarea",
        &[("font", "inherit"), ("color", "inherit")],
    );
    s.rule(
        "button",
        &[
            ("cursor", "pointer"),
            ("border", "0"),
            ("background", "none"),
            ("-webkit-tap-highlight-color", "transparent"),
        ],
    );
    s.rule("a", &[("color", "var(--accent-2)")]);
    s.rule(
        ":focus-visible",
        &[
            ("outline", "2px solid var(--accent)"),
            ("outline-offset", "2px"),
        ],
    );
    s.rule("#root", &[("height", "var(--app-height)")]);
    s.rule(
        ".boot",
        &[
            ("display", "grid"),
            ("place-content", "center"),
            ("justify-items", "center"),
            ("gap", "0.5rem"),
            ("height", "100%"),
            ("color", "var(--muted)"),
        ],
    );
    s.rule(
        ".boot img",
        &[
            ("border-radius", "18px"),
            ("animation", "pulse 1.4s ease-in-out infinite"),
        ],
    );
    s.keyframes("pulse", |s| {
        s.rule("50%", &[("transform", "scale(0.94)"), ("opacity", "0.7")]);
    });
    // ---- Disposition ----------------------------------------------------
    s.rule(
        ".layout",
        &[
            ("display", "grid"),
            (
                "grid-template-columns",
                "calc(72px + var(--safe-left)) 248px minmax(0, 1fr) 0",
            ),
            ("height", "100%"),
            ("transition", "grid-template-columns 0.2s ease"),
        ],
    );
    s.rule(
        ".layout.members-open",
        &[(
            "grid-template-columns",
            "calc(72px + var(--safe-left)) 248px minmax(0, 1fr) calc(248px + var(--safe-right))",
        )],
    );
    s.rule(".scrim", &[("display", "none")]);
    s.rule(
        ".rail",
        &[
            ("background", "var(--rail)"),
            (
                "padding",
                "calc(12px + var(--safe-top)) 0 calc(12px + var(--safe-bottom)) var(--safe-left)",
            ),
            ("display", "flex"),
            ("flex-direction", "column"),
            ("align-items", "center"),
            ("gap", "8px"),
            ("overflow-y", "auto"),
            ("scrollbar-width", "none"),
        ],
    );
    s.rule(".rail::-webkit-scrollbar", &[("display", "none")]);
    s.rule(
        ".rail hr",
        &[
            ("width", "32px"),
            ("border", "0"),
            ("border-top", "2px solid var(--line)"),
            ("margin", "2px 0"),
        ],
    );
    s.rule(
        ".home",
        &[
            ("display", "grid"),
            ("place-items", "center"),
            ("width", "48px"),
            ("height", "48px"),
            ("border-radius", "16px"),
            ("background", "var(--elev)"),
        ],
    );
    s.rule(
        ".guild",
        &[
            ("position", "relative"),
            ("flex", "none"),
            ("width", "48px"),
            ("height", "48px"),
            ("border-radius", "50%"),
            ("background", "hsl(var(--h, 260) 55% 42%)"),
            ("color", "#fff"),
            ("font-weight", "700"),
            ("font-size", "15px"),
            ("transition", "border-radius 0.15s, transform 0.15s"),
        ],
    );
    s.rule(".guild:hover, .guild.active", &[("border-radius", "16px")]);
    s.rule(
        ".guild.active::before",
        &[
            ("content", r#""""#),
            ("position", "absolute"),
            ("left", "-12px"),
            ("top", "8px"),
            ("bottom", "8px"),
            ("width", "4px"),
            ("border-radius", "0 4px 4px 0"),
            ("background", "var(--fg)"),
        ],
    );
    s.rule(
        ".guild.add",
        &[
            ("background", "var(--elev)"),
            ("color", "var(--accent-2)"),
            ("font-size", "26px"),
            ("font-weight", "400"),
        ],
    );
    s.rule(
        ".guild .badge",
        &[
            ("position", "absolute"),
            ("right", "-4px"),
            ("bottom", "-4px"),
            ("border", "3px solid var(--rail)"),
        ],
    );
    s.rule(
        ".badge",
        &[
            ("min-width", "20px"),
            ("height", "20px"),
            ("padding", "0 6px"),
            ("border-radius", "10px"),
            ("background", "var(--danger)"),
            ("color", "#fff"),
            ("font-size", "12px"),
            ("font-weight", "700"),
            ("line-height", "20px"),
            ("text-align", "center"),
        ],
    );
    s.rule(
        ".sidebar",
        &[
            ("background", "var(--side)"),
            ("display", "flex"),
            ("flex-direction", "column"),
            ("min-width", "0"),
            ("padding-top", "var(--safe-top)"),
        ],
    );
    s.rule(".guild-head", &[("border-bottom", "1px solid var(--line)")]);
    s.rule(
        ".guild-name",
        &[
            ("width", "100%"),
            ("height", "52px"),
            ("padding", "0 16px"),
            ("display", "flex"),
            ("align-items", "center"),
            ("justify-content", "space-between"),
            ("font-weight", "700"),
            ("text-align", "left"),
        ],
    );
    s.rule(
        ".guild-name span:first-child",
        &[
            ("overflow", "hidden"),
            ("text-overflow", "ellipsis"),
            ("white-space", "nowrap"),
        ],
    );
    s.rule(".guild-name:hover", &[("background", "var(--hover)")]);
    s.rule(
        ".channels",
        &[
            ("flex", "1"),
            ("overflow-y", "auto"),
            ("padding", "12px 8px"),
        ],
    );
    s.rule(
        ".channels ul, .members ul",
        &[("list-style", "none"), ("margin", "0"), ("padding", "0")],
    );
    s.rule(
        ".section-title",
        &[
            ("display", "flex"),
            ("align-items", "center"),
            ("justify-content", "space-between"),
            ("padding", "8px 8px 4px"),
            ("color", "var(--muted)"),
            ("font-size", "12px"),
            ("font-weight", "700"),
            ("text-transform", "uppercase"),
            ("letter-spacing", "0.04em"),
        ],
    );
    s.rule(
        ".channel",
        &[
            ("width", "100%"),
            ("display", "flex"),
            ("align-items", "center"),
            ("gap", "6px"),
            ("padding", "8px 8px"),
            ("border-radius", "8px"),
            ("color", "var(--muted)"),
            ("text-align", "left"),
            ("min-height", "40px"),
        ],
    );
    s.rule(
        ".channel:hover",
        &[("background", "var(--hover)"), ("color", "var(--fg)")],
    );
    s.rule(
        ".channel.active",
        &[("background", "var(--elev)"), ("color", "var(--fg)")],
    );
    s.rule(
        ".channel.unread",
        &[("color", "var(--fg)"), ("font-weight", "700")],
    );
    s.rule(
        ".channel .hash",
        &[
            ("color", "var(--faint)"),
            ("font-size", "20px"),
            ("line-height", "1"),
            ("width", "18px"),
            ("text-align", "center"),
        ],
    );
    s.rule(
        ".channel .name",
        &[
            ("flex", "1"),
            ("overflow", "hidden"),
            ("text-overflow", "ellipsis"),
            ("white-space", "nowrap"),
        ],
    );
    s.rule(
        ".channel .gear",
        &[
            ("opacity", "0"),
            ("padding", "0 4px"),
            ("color", "var(--muted)"),
        ],
    );
    s.rule(
        ".channel:hover .gear, .channel.active .gear",
        &[("opacity", "1")],
    );
    s.rule(
        ".me",
        &[
            ("display", "flex"),
            ("align-items", "center"),
            ("gap", "10px"),
            ("padding", "10px 10px calc(10px + var(--safe-bottom))"),
            ("background", "var(--rail)"),
        ],
    );
    s.rule(
        ".me-text",
        &[("flex", "1"), ("min-width", "0"), ("display", "grid")],
    );
    s.rule(
        ".me-text strong, .me-text small",
        &[
            ("overflow", "hidden"),
            ("text-overflow", "ellipsis"),
            ("white-space", "nowrap"),
        ],
    );
    s.rule(
        ".me-text small",
        &[("color", "var(--muted)"), ("font-size", "12px")],
    );
    s.rule(
        ".icon-btn",
        &[
            ("display", "inline-grid"),
            ("place-items", "center"),
            ("min-width", "36px"),
            ("height", "36px"),
            ("border-radius", "8px"),
            ("color", "var(--muted)"),
            ("font-size", "18px"),
        ],
    );
    s.rule(
        ".icon-btn:hover",
        &[("background", "var(--hover)"), ("color", "var(--fg)")],
    );
    s.rule(
        ".avatar",
        &[
            ("flex", "none"),
            ("display", "grid"),
            ("place-items", "center"),
            ("width", "40px"),
            ("height", "40px"),
            ("border-radius", "50%"),
            ("background", "hsl(var(--h, 260) 55% 45%)"),
            ("color", "#fff"),
            ("font-weight", "700"),
            ("font-size", "14px"),
            ("user-select", "none"),
        ],
    );
    // ---- Discussion -----------------------------------------------------
    s.rule(
        ".chat",
        &[
            ("background", "var(--chat)"),
            ("display", "flex"),
            ("flex-direction", "column"),
            ("min-width", "0"),
            ("min-height", "0"),
            ("padding-top", "var(--safe-top)"),
        ],
    );
    s.rule(
        ".chat-head",
        &[
            ("display", "flex"),
            ("align-items", "center"),
            ("gap", "8px"),
            ("height", "52px"),
            ("padding", "0 12px 0 16px"),
            ("border-bottom", "1px solid var(--line)"),
            ("flex", "none"),
        ],
    );
    s.rule(
        ".chat-head .hash",
        &[("color", "var(--faint)"), ("font-size", "22px")],
    );
    s.rule(".chat-head .title", &[("white-space", "nowrap")]);
    s.rule(
        ".chat-head .topic",
        &[
            ("color", "var(--muted)"),
            ("font-size", "14px"),
            ("overflow", "hidden"),
            ("text-overflow", "ellipsis"),
            ("white-space", "nowrap"),
            ("border-left", "1px solid var(--line)"),
            ("padding-left", "10px"),
            ("margin-left", "4px"),
        ],
    );
    s.rule(".chat-head .spacer", &[("flex", "1")]);
    s.rule(
        ".chat-head .lock",
        &[("font-size", "15px"), ("cursor", "help")],
    );
    s.rule(".chat-head .menu", &[("display", "none")]);
    s.rule(
        ".messages",
        &[
            ("flex", "1"),
            ("overflow-y", "auto"),
            ("padding", "8px 0 16px"),
            ("overscroll-behavior", "contain"),
            ("-webkit-overflow-scrolling", "touch"),
        ],
    );
    s.rule(
        ".loading",
        &[
            ("text-align", "center"),
            ("color", "var(--muted)"),
            ("padding", "32px"),
        ],
    );
    s.rule(".channel-start", &[("padding", "32px 16px 16px")]);
    s.rule(
        ".channel-start .big-hash",
        &[
            ("display", "grid"),
            ("place-items", "center"),
            ("width", "64px"),
            ("height", "64px"),
            ("border-radius", "50%"),
            ("background", "var(--elev)"),
            ("font-size", "38px"),
            ("color", "var(--muted)"),
        ],
    );
    s.rule(
        ".channel-start h2",
        &[("margin", "12px 0 4px"), ("font-size", "26px")],
    );
    s.rule(
        ".channel-start p",
        &[
            ("margin", "0"),
            ("color", "var(--muted)"),
            ("max-width", "60ch"),
        ],
    );
    s.rule(
        ".day",
        &[
            ("display", "flex"),
            ("align-items", "center"),
            ("gap", "8px"),
            ("margin", "16px 16px 4px"),
            ("color", "var(--muted)"),
            ("font-size", "12px"),
            ("font-weight", "600"),
        ],
    );
    s.rule(
        ".day::before, .day::after",
        &[
            ("content", r#""""#),
            ("flex", "1"),
            ("border-top", "1px solid var(--line)"),
        ],
    );
    s.rule(
        ".message",
        &[
            ("position", "relative"),
            ("display", "grid"),
            ("grid-template-columns", "40px minmax(0, 1fr)"),
            ("column-gap", "14px"),
            ("padding", "6px 48px 6px 16px"),
            ("margin-top", "10px"),
        ],
    );
    s.rule(
        ".message.grouped",
        &[
            ("margin-top", "0"),
            ("padding-top", "2px"),
            ("padding-bottom", "2px"),
        ],
    );
    s.rule(
        ".message:hover, .message.show-actions",
        &[(
            "background",
            "color-mix(in srgb, var(--hover) 55%, transparent)",
        )],
    );
    s.rule(".message.flash", &[("animation", "flash 1.4s ease")]);
    s.keyframes("flash", |s| {
        s.rule(
            "0%, 40%",
            &[(
                "background",
                "color-mix(in srgb, var(--accent) 25%, transparent)",
            )],
        );
    });
    s.rule(".message .body", &[("min-width", "0")]);
    s.rule(
        ".meta",
        &[
            ("display", "flex"),
            ("align-items", "baseline"),
            ("gap", "8px"),
            ("flex-wrap", "wrap"),
        ],
    );
    s.rule(
        ".author",
        &[
            ("color", "hsl(var(--h, 260) 75% 72%)"),
            ("font-weight", "600"),
        ],
    );
    s.media("(prefers-color-scheme: light)", |s| {
        s.rule(".author", &[("color", "hsl(var(--h, 260) 60% 38%)")]);
    });
    s.rule(
        ".meta time",
        &[("color", "var(--faint)"), ("font-size", "12px")],
    );
    s.rule(
        ".hover-time",
        &[
            ("color", "var(--faint)"),
            ("font-size", "11px"),
            ("text-align", "right"),
            ("padding-top", "3px"),
            ("opacity", "0"),
        ],
    );
    s.rule(".message:hover .hover-time", &[("opacity", "1")]);
    s.rule(
        ".text",
        &[("overflow-wrap", "anywhere"), ("white-space", "normal")],
    );
    s.rule(
        ".text.jumbo",
        &[("font-size", "42px"), ("line-height", "1.2")],
    );
    s.rule(
        ".text code, pre",
        &[
            ("font-family", "var(--mono)"),
            ("font-size", "14px"),
            ("background", "var(--rail)"),
            ("border-radius", "6px"),
        ],
    );
    s.rule(".text code", &[("padding", "1px 5px")]);
    s.rule(
        "pre",
        &[
            ("padding", "10px 12px"),
            ("margin", "6px 0"),
            ("overflow-x", "auto"),
            ("white-space", "pre"),
            ("border", "1px solid var(--line)"),
        ],
    );
    s.rule("pre code", &[("background", "none"), ("padding", "0")]);
    // ---- Le code partagé : blocs colorés ---------------------------------
    s.rule(
        ":root",
        &[
            ("--tk-kw", "#c678dd"),
            ("--tk-ty", "#e5c07b"),
            ("--tk-lit", "#d19a66"),
            ("--tk-str", "#98c379"),
            ("--tk-num", "#d19a66"),
            ("--tk-com", "#7f848e"),
            ("--tk-fn", "#61afef"),
            ("--tk-mac", "#56b6c2"),
            ("--tk-attr", "#d7ba7d"),
            ("--tk-var", "#e06c75"),
            ("--tk-op", "#56b6c2"),
            ("--tk-tag", "#e06c75"),
            ("--tk-key", "#e06c75"),
            ("--tk-add", "#98c379"),
            ("--tk-del", "#e06c75"),
            ("--tk-meta", "#61afef"),
            ("--code-fg", "#abb2bf"),
            ("--code-bg", "#1e1b2b"),
            ("--code-head", "#25213a"),
        ],
    );
    s.media("(prefers-color-scheme: light)", |s| {
        s.rule(
            ":root",
            &[
                ("--tk-kw", "#cf222e"),
                ("--tk-ty", "#953800"),
                ("--tk-lit", "#0550ae"),
                ("--tk-str", "#0a3069"),
                ("--tk-num", "#0550ae"),
                ("--tk-com", "#6e7781"),
                ("--tk-fn", "#8250df"),
                ("--tk-mac", "#0550ae"),
                ("--tk-attr", "#6639ba"),
                ("--tk-var", "#953800"),
                ("--tk-op", "#cf222e"),
                ("--tk-tag", "#116329"),
                ("--tk-key", "#0550ae"),
                ("--tk-add", "#116329"),
                ("--tk-del", "#82071e"),
                ("--tk-meta", "#8250df"),
                ("--code-fg", "#1f2328"),
                ("--code-bg", "#f6f8fa"),
                ("--code-head", "#eef1f4"),
            ],
        );
    });
    s.rule(
        ".codeblock",
        &[
            ("margin", "6px 0"),
            ("border", "1px solid var(--line)"),
            ("border-radius", "10px"),
            ("overflow", "hidden"),
            ("background", "var(--code-bg)"),
            ("max-width", "min(780px, 100%)"),
        ],
    );
    s.rule(
        ".code-head",
        &[
            ("display", "flex"),
            ("align-items", "center"),
            ("gap", "10px"),
            ("padding", "5px 6px 5px 12px"),
            ("background", "var(--code-head)"),
            ("border-bottom", "1px solid var(--line)"),
            ("font-size", "12px"),
            ("color", "var(--muted)"),
        ],
    );
    s.rule(
        ".code-lang",
        &[
            ("font-weight", "700"),
            ("color", "var(--fg)"),
            ("letter-spacing", "0.02em"),
        ],
    );
    s.rule(".code-head .spacer", &[("flex", "1")]);
    s.rule(
        ".code-copy",
        &[
            ("padding", "4px 10px"),
            ("border-radius", "6px"),
            ("border", "1px solid var(--line)"),
            ("background", "transparent"),
            ("color", "var(--muted)"),
            ("font-size", "12px"),
            ("font-weight", "600"),
        ],
    );
    s.rule(
        ".code-copy:hover",
        &[("color", "var(--fg)"), ("border-color", "var(--accent)")],
    );
    s.rule(
        "pre.code",
        &[
            ("margin", "0"),
            ("padding", "10px 0"),
            ("border", "0"),
            ("border-radius", "0"),
            ("background", "transparent"),
            ("color", "var(--code-fg)"),
            ("counter-reset", "line"),
            ("max-height", "520px"),
            ("overflow", "auto"),
            ("tab-size", "4"),
            ("font-size", "13.5px"),
        ],
    );
    s.rule(
        "pre.code .line",
        &[
            ("display", "block"),
            ("min-height", "1.55em"),
            ("line-height", "1.55"),
            ("padding-right", "16px"),
        ],
    );
    s.rule(
        "pre.code .line::before",
        &[
            ("counter-increment", "line"),
            ("content", "counter(line)"),
            ("display", "inline-block"),
            ("min-width", "3ch"),
            ("margin-right", "14px"),
            ("padding-left", "10px"),
            ("text-align", "right"),
            ("color", "var(--tk-com)"),
            ("opacity", "0.7"),
            ("user-select", "none"),
            ("-webkit-user-select", "none"),
        ],
    );
    s.rule(".tk-kw", &[("color", "var(--tk-kw)")]);
    s.rule(".tk-ty", &[("color", "var(--tk-ty)")]);
    s.rule(".tk-lit", &[("color", "var(--tk-lit)")]);
    s.rule(".tk-str", &[("color", "var(--tk-str)")]);
    s.rule(".tk-num", &[("color", "var(--tk-num)")]);
    s.rule(".tk-com", &[("color", "var(--tk-com)")]);
    s.rule(".tk-fn", &[("color", "var(--tk-fn)")]);
    s.rule(".tk-mac", &[("color", "var(--tk-mac)")]);
    s.rule(".tk-attr", &[("color", "var(--tk-attr)")]);
    s.rule(".tk-var", &[("color", "var(--tk-var)")]);
    s.rule(".tk-op", &[("color", "var(--tk-op)")]);
    s.rule(".tk-tag", &[("color", "var(--tk-tag)")]);
    s.rule(".tk-key", &[("color", "var(--tk-key)")]);
    s.rule(".tk-add", &[("color", "var(--tk-add)")]);
    s.rule(".tk-del", &[("color", "var(--tk-del)")]);
    s.rule(".tk-meta", &[("color", "var(--tk-meta)")]);
    s.rule(".tk-com", &[("font-style", "italic")]);
    s.rule(
        ".text pre code",
        &[
            ("background", "none"),
            ("padding", "0"),
            ("font-size", "inherit"),
        ],
    );
    s.rule(".tk-kw, .tk-tag", &[("font-weight", "600")]);
    s.rule(
        ".code-btn",
        &[
            ("font-family", "var(--mono)"),
            ("font-size", "13px"),
            ("font-weight", "700"),
        ],
    );
    s.rule(
        "blockquote",
        &[
            ("margin", "2px 0"),
            ("padding-left", "10px"),
            ("border-left", "4px solid var(--line)"),
            ("color", "var(--muted)"),
        ],
    );
    s.rule(
        ".locked",
        &[
            ("color", "var(--faint)"),
            ("font-style", "italic"),
            ("margin", "0"),
        ],
    );
    s.rule(
        ".edited",
        &[
            ("color", "var(--faint)"),
            ("font-size", "11px"),
            ("margin-left", "4px"),
        ],
    );
    s.rule(
        ".image",
        &[
            ("display", "block"),
            ("padding", "0"),
            ("margin-top", "4px"),
            ("border-radius", "10px"),
            ("overflow", "hidden"),
            ("max-width", "min(400px, 100%)"),
        ],
    );
    s.rule(".image", &[("position", "relative")]);
    s.rule(
        ".image.pending img",
        &[("opacity", "0.45"), ("filter", "blur(6px)")],
    );
    s.rule(
        ".image .checking",
        &[
            ("position", "absolute"),
            ("inset", "0"),
            ("display", "grid"),
            ("place-items", "center"),
            ("font-weight", "700"),
            ("color", "var(--fg)"),
        ],
    );
    s.rule(
        ".image img",
        &[
            ("display", "block"),
            ("max-width", "100%"),
            ("max-height", "360px"),
            ("object-fit", "contain"),
            ("background", "var(--rail)"),
        ],
    );
    s.rule(
        ".reply-ref",
        &[
            ("grid-column", "2"),
            ("display", "flex"),
            ("align-items", "center"),
            ("gap", "6px"),
            ("font-size", "13px"),
            ("color", "var(--muted)"),
            ("cursor", "pointer"),
            ("overflow", "hidden"),
            ("white-space", "nowrap"),
            ("margin-bottom", "2px"),
        ],
    );
    s.rule(
        ".reply-ref span:last-child",
        &[("overflow", "hidden"), ("text-overflow", "ellipsis")],
    );
    s.rule(
        ".reply-ref .curve",
        &[
            ("width", "26px"),
            ("height", "10px"),
            ("margin-left", "-34px"),
            ("border", "2px solid var(--line)"),
            ("border-right", "0"),
            ("border-bottom", "0"),
            ("border-radius", "8px 0 0 0"),
            ("align-self", "flex-end"),
        ],
    );
    s.rule(
        ".actions",
        &[
            ("position", "absolute"),
            ("right", "12px"),
            ("top", "-14px"),
            ("display", "none"),
            ("gap", "2px"),
            ("padding", "2px"),
            ("background", "var(--elev)"),
            ("border", "1px solid var(--line)"),
            ("border-radius", "8px"),
            ("box-shadow", "0 4px 14px #0004"),
            ("z-index", "2"),
        ],
    );
    s.rule(
        ".message:hover .actions, .message.show-actions .actions, .message:focus-within .actions",
        &[("display", "flex")],
    );
    s.rule(
        ".actions button",
        &[
            ("width", "34px"),
            ("height", "32px"),
            ("border-radius", "6px"),
            ("color", "var(--muted)"),
        ],
    );
    s.rule(
        ".actions button:hover",
        &[("background", "var(--hover)"), ("color", "var(--fg)")],
    );
    s.rule(".actions .danger:hover", &[("color", "var(--danger)")]);
    s.rule(
        ".typing",
        &[
            ("height", "22px"),
            ("padding", "0 16px"),
            ("color", "var(--muted)"),
            ("font-size", "13px"),
            ("display", "flex"),
            ("align-items", "center"),
            ("gap", "4px"),
            ("flex", "none"),
        ],
    );
    s.rule(".dots", &[("display", "inline-flex"), ("gap", "3px")]);
    s.rule(
        ".dots i",
        &[
            ("width", "6px"),
            ("height", "6px"),
            ("border-radius", "50%"),
            ("background", "var(--muted)"),
            ("animation", "dot 1.2s infinite"),
        ],
    );
    s.rule(".dots i:nth-child(2)", &[("animation-delay", "0.15s")]);
    s.rule(".dots i:nth-child(3)", &[("animation-delay", "0.3s")]);
    s.keyframes("dot", |s| {
        s.rule(
            "30%",
            &[("transform", "translateY(-4px)"), ("opacity", "0.5")],
        );
    });
    s.rule(
        ".composer",
        &[
            ("padding", "0 16px calc(16px + var(--safe-bottom))"),
            ("flex", "none"),
        ],
    );
    s.rule(
        ".bar",
        &[
            ("display", "flex"),
            ("align-items", "center"),
            ("justify-content", "space-between"),
            ("gap", "8px"),
            ("background", "var(--elev)"),
            ("border-radius", "var(--radius) var(--radius) 0 0"),
            ("padding", "4px 4px 4px 14px"),
            ("font-size", "14px"),
            ("color", "var(--muted)"),
        ],
    );
    s.rule(".bar + *", &[("border-radius", "0")]);
    s.rule(
        ".input-row",
        &[
            ("display", "flex"),
            ("align-items", "flex-end"),
            ("gap", "6px"),
            ("padding", "6px"),
            ("background", "var(--elev)"),
            ("border-radius", "var(--radius)"),
            ("border", "1px solid var(--line)"),
        ],
    );
    s.rule(
        "#bar:not(:empty) + .input-row",
        &[
            ("border-top-left-radius", "0"),
            ("border-top-right-radius", "0"),
        ],
    );
    s.rule(
        ".input-row textarea",
        &[
            ("flex", "1"),
            ("min-height", "40px"),
            ("max-height", "200px"),
            ("resize", "none"),
            ("border", "0"),
            ("background", "transparent"),
            ("outline", "none"),
            ("padding", "9px 6px"),
            ("font-size", "16px"),
            ("line-height", "1.4"),
        ],
    );
    s.rule(".attach", &[("font-size", "22px"), ("height", "40px")]);
    s.rule(
        ".send",
        &[
            ("width", "40px"),
            ("height", "40px"),
            ("border-radius", "10px"),
            ("background", "var(--accent)"),
            ("color", "var(--accent-fg)"),
            ("font-size", "16px"),
            ("flex", "none"),
        ],
    );
    // ---- Membres ----------------------------------------------------------
    s.rule(
        ".members",
        &[
            ("background", "var(--side)"),
            ("overflow", "hidden auto"),
            (
                "padding",
                "var(--safe-top) var(--safe-right) var(--safe-bottom) 0",
            ),
            ("min-width", "0"),
        ],
    );
    s.rule(
        ".members-head",
        &[
            ("display", "flex"),
            ("align-items", "center"),
            ("justify-content", "space-between"),
            ("height", "52px"),
            ("padding", "0 8px 0 16px"),
            ("border-bottom", "1px solid var(--line)"),
        ],
    );
    s.rule(
        ".member",
        &[
            ("display", "flex"),
            ("align-items", "center"),
            ("gap", "10px"),
            ("padding", "6px 16px"),
            ("opacity", "0.55"),
        ],
    );
    s.rule(".member.on", &[("opacity", "1")]);
    s.rule(
        ".member .name",
        &[
            ("overflow", "hidden"),
            ("text-overflow", "ellipsis"),
            ("white-space", "nowrap"),
        ],
    );
    s.rule(".member small", &[("color", "var(--muted)")]);
    s.rule(".avatar-wrap", &[("position", "relative")]);
    s.rule(
        ".avatar-wrap .avatar",
        &[("width", "34px"), ("height", "34px"), ("font-size", "12px")],
    );
    s.rule(
        ".avatar-wrap .dot",
        &[
            ("position", "absolute"),
            ("right", "-2px"),
            ("bottom", "-2px"),
            ("width", "14px"),
            ("height", "14px"),
            ("border-radius", "50%"),
            ("border", "3px solid var(--side)"),
            ("background", "var(--faint)"),
        ],
    );
    s.rule(".member.on .dot", &[("background", "var(--accent-2)")]);
    s.rule(
        ".fp",
        &[
            ("margin", "16px"),
            ("padding", "12px"),
            ("border-radius", "var(--radius)"),
            ("background", "var(--chat)"),
            ("display", "grid"),
            ("gap", "4px"),
        ],
    );
    s.rule(
        ".fp code",
        &[
            ("font-family", "var(--mono)"),
            ("font-size", "15px"),
            ("letter-spacing", "0.04em"),
        ],
    );
    s.rule(".muted", &[("color", "var(--muted)")]);
    s.rule(".small", &[("font-size", "13px")]);
    // ---- Accueil ----------------------------------------------------------
    s.rule(
        ".welcome",
        &[
            ("display", "grid"),
            ("place-items", "center"),
            ("min-height", "100%"),
            (
                "padding",
                "calc(24px + var(--safe-top)) 16px calc(24px + var(--safe-bottom))",
            ),
            ("overflow-y", "auto"),
        ],
    );
    s.rule(
        ".welcome-card",
        &[
            ("width", "min(440px, 100%)"),
            ("background", "var(--side)"),
            ("border", "1px solid var(--line)"),
            ("border-radius", "20px"),
            ("padding", "28px"),
            ("display", "grid"),
            ("gap", "14px"),
            ("text-align", "center"),
            ("justify-items", "center"),
        ],
    );
    s.rule(".welcome-card img", &[("border-radius", "18px")]);
    s.rule(
        ".welcome-card h1",
        &[("margin", "0"), ("font-size", "26px")],
    );
    s.rule(".welcome-card p", &[("margin", "0")]);
    s.rule(
        ".welcome-card .field",
        &[("width", "100%"), ("text-align", "left")],
    );
    // ---- Formulaires et fenêtres -------------------------------------------
    s.rule(".field", &[("display", "grid"), ("gap", "6px")]);
    s.rule(
        ".field > span",
        &[
            ("font-size", "12px"),
            ("font-weight", "700"),
            ("text-transform", "uppercase"),
            ("letter-spacing", "0.04em"),
            ("color", "var(--muted)"),
        ],
    );
    s.rule(
        ".field input, input.link",
        &[
            ("width", "100%"),
            ("min-height", "44px"),
            ("padding", "10px 12px"),
            ("font-size", "16px"),
            ("border-radius", "10px"),
            ("border", "1px solid var(--line)"),
            ("background", "var(--rail)"),
            ("color", "var(--fg)"),
        ],
    );
    s.rule(
        ".check",
        &[
            ("display", "flex"),
            ("gap", "10px"),
            ("align-items", "center"),
            ("min-height", "44px"),
        ],
    );
    s.rule(
        ".check input",
        &[
            ("width", "20px"),
            ("height", "20px"),
            ("accent-color", "var(--accent)"),
        ],
    );
    s.rule(
        ".modal button, .welcome-card button",
        &[
            ("min-height", "44px"),
            ("padding", "0 18px"),
            ("border-radius", "10px"),
            ("background", "var(--elev)"),
            ("border", "1px solid var(--line)"),
            ("font-weight", "600"),
        ],
    );
    s.rule(
        ".modal button.primary, .welcome-card button.primary",
        &[
            ("background", "var(--accent)"),
            ("color", "var(--accent-fg)"),
            ("border-color", "transparent"),
        ],
    );
    s.rule(".welcome-card button.primary", &[("width", "100%")]);
    s.rule(".modal button.ghost", &[("background", "transparent")]);
    s.rule(
        ".modal button.danger, .menu .danger",
        &[("color", "var(--danger)")],
    );
    s.rule(".modal button:hover", &[("filter", "brightness(1.1)")]);
    s.rule(
        ".modal",
        &[
            ("width", "min(480px, calc(100vw - 24px))"),
            ("max-height", "calc(var(--app-height) - 24px)"),
            ("padding", "0"),
            ("border", "1px solid var(--line)"),
            ("border-radius", "18px"),
            ("background", "var(--side)"),
            ("color", "var(--fg)"),
            ("box-shadow", "0 24px 60px #0008"),
        ],
    );
    s.rule(
        ".modal::backdrop",
        &[
            ("background", "#0009"),
            ("backdrop-filter", "blur(3px)"),
            ("-webkit-backdrop-filter", "blur(3px)"),
        ],
    );
    s.rule(
        ".modal-body",
        &[("padding", "22px"), ("display", "grid"), ("gap", "16px")],
    );
    s.rule(
        ".modal h2",
        &[
            ("margin", "0"),
            ("font-size", "20px"),
            ("overflow-wrap", "anywhere"),
        ],
    );
    s.rule(".modal h3", &[("margin", "0 0 8px"), ("font-size", "16px")]);
    s.rule(".modal p", &[("margin", "0")]);
    s.rule(
        ".modal-actions",
        &[
            ("display", "flex"),
            ("flex-wrap", "wrap"),
            ("justify-content", "flex-end"),
            ("gap", "8px"),
        ],
    );
    s.rule(".stack", &[("display", "grid"), ("gap", "14px")]);
    s.rule(
        ".choice",
        &[
            ("display", "grid"),
            ("gap", "10px"),
            ("padding", "14px"),
            ("background", "var(--chat)"),
            ("border-radius", "14px"),
        ],
    );
    s.rule(".menu", &[("display", "grid"), ("gap", "6px")]);
    s.rule(
        ".menu button",
        &[
            ("text-align", "left"),
            ("background", "var(--chat)"),
            ("border-color", "transparent"),
        ],
    );
    s.rule(
        "details summary",
        &[
            ("cursor", "pointer"),
            ("color", "var(--muted)"),
            ("padding", "6px 0"),
        ],
    );
    s.rule("details > *:not(summary)", &[("margin-top", "8px")]);
    s.rule(
        ".fp-inline code, .fp-big code",
        &[("font-family", "var(--mono)")],
    );
    s.rule(
        ".fp-big",
        &[
            ("display", "grid"),
            ("gap", "4px"),
            ("padding", "14px"),
            ("background", "var(--chat)"),
            ("border-radius", "14px"),
            ("text-align", "center"),
        ],
    );
    s.rule(
        ".fp-big code",
        &[("font-size", "22px"), ("letter-spacing", "0.06em")],
    );
    s.rule(
        ".lightbox",
        &[
            ("padding", "0"),
            ("border", "0"),
            ("background", "transparent"),
            ("max-width", "100vw"),
            ("max-height", "100vh"),
        ],
    );
    s.rule(".lightbox::backdrop", &[("background", "#000d")]);
    s.rule(
        ".lightbox img",
        &[
            ("max-width", "96vw"),
            ("max-height", "92vh"),
            ("object-fit", "contain"),
            ("display", "block"),
        ],
    );
    s.rule(
        ".lightbox .close",
        &[
            ("position", "fixed"),
            ("top", "calc(12px + var(--safe-top))"),
            ("right", "12px"),
            ("background", "#0008"),
            ("color", "#fff"),
        ],
    );
    s.rule(
        "#toasts",
        &[
            ("position", "fixed"),
            ("left", "50%"),
            ("bottom", "calc(90px + var(--safe-bottom))"),
            ("transform", "translateX(-50%)"),
            ("display", "grid"),
            ("gap", "8px"),
            ("z-index", "50"),
            ("width", "min(420px, calc(100vw - 32px))"),
            ("pointer-events", "none"),
        ],
    );
    s.rule(
        ".toast",
        &[
            ("padding", "10px 14px"),
            ("border-radius", "12px"),
            ("background", "var(--elev)"),
            ("border", "1px solid var(--line)"),
            ("box-shadow", "0 8px 24px #0006"),
            ("animation", "up 0.2s ease"),
            ("text-align", "center"),
        ],
    );
    s.rule(".toast.error", &[("border-color", "var(--danger)")]);
    s.rule(
        ".toast.out",
        &[("opacity", "0"), ("transition", "opacity 0.3s")],
    );
    s.keyframes("up", |s| {
        s.rule(
            "from",
            &[("transform", "translateY(8px)"), ("opacity", "0")],
        );
    });
    // ---- Téléphones et petites fenêtres ------------------------------------
    s.media("(max-width: 860px)", |s| {
        s.rule(
            ".layout, .layout.members-open",
            &[
                ("grid-template-columns", "minmax(0, 1fr)"),
                ("position", "relative"),
                ("overflow", "hidden"),
                // Sans `clip`, le navigateur fait défiler la disposition
                // vers un panneau replié (au focus, pendant un appel).
                ("overflow", "clip"),
            ],
        );
        s.rule(
            ".chat",
            &[
                ("grid-column", "1"),
                ("grid-row", "1"),
                ("height", "var(--app-height)"),
            ],
        );
        s.rule(".chat-head .menu", &[("display", "inline-grid")]);
        s.rule(".chat-head .topic", &[("display", "none")]);
        s.rule(
            ".rail, .sidebar, .members",
            &[
                ("position", "absolute"),
                ("top", "0"),
                ("bottom", "0"),
                ("z-index", "10"),
                ("transition", "transform 0.22s ease"),
                ("height", "var(--app-height)"),
            ],
        );
        s.rule(
            ".rail",
            &[
                ("left", "0"),
                ("width", "calc(72px + var(--safe-left))"),
                ("transform", "translateX(calc(-100% - 260px))"),
            ],
        );
        s.rule(
            ".sidebar",
            &[
                ("left", "calc(72px + var(--safe-left))"),
                (
                    "width",
                    "min(280px, calc(100vw - 72px - var(--safe-left) - 40px))",
                ),
                ("transform", "translateX(calc(-100% - 80px))"),
            ],
        );
        s.rule(
            ".members",
            &[
                ("right", "0"),
                ("width", "min(300px, 85vw)"),
                ("transform", "translateX(105%)"),
            ],
        );
        s.rule(
            ".drawer-open .rail, .drawer-open .sidebar",
            &[("transform", "none")],
        );
        s.rule(".members-open .members", &[("transform", "none")]);
        s.rule(
            ".scrim",
            &[
                ("display", "block"),
                ("position", "absolute"),
                ("inset", "0"),
                ("z-index", "5"),
                ("background", "#0008"),
                ("opacity", "0"),
                ("pointer-events", "none"),
                ("transition", "opacity 0.2s"),
            ],
        );
        s.rule(
            ".drawer-open .scrim, .members-open .scrim",
            &[("opacity", "1"), ("pointer-events", "auto")],
        );
        s.rule(".message", &[("padding-right", "12px")]);
        s.rule(".actions", &[("top", "auto"), ("bottom", "-12px")]);
    });
    s.media("(min-width: 861px)", |s| {
        s.rule(".members-head button", &[("display", "none")]);
    });
    s.media("(prefers-reduced-motion: reduce)", |s| {
        s.rule(
            "*, *::before, *::after",
            &[
                ("animation", "none !important"),
                ("transition", "none !important"),
            ],
        );
    });
    // ---- Salons vocaux et appels -----------------------------------------
    s.rule(
        ".voice-members",
        &[
            ("list-style", "none"),
            ("margin", "0 0 4px"),
            ("padding", "0 0 0 30px"),
        ],
    );
    s.rule(
        ".voice-members li",
        &[
            ("display", "flex"),
            ("align-items", "center"),
            ("gap", "8px"),
            ("padding", "3px 6px"),
            ("border-radius", "6px"),
            ("font-size", "14px"),
            ("color", "var(--muted)"),
        ],
    );
    s.rule(
        ".voice-members li.speaking .avatar",
        &[("box-shadow", "0 0 0 2px var(--accent-2)")],
    );
    s.rule(
        ".avatar.tiny",
        &[("width", "22px"), ("height", "22px"), ("font-size", "10px")],
    );
    s.rule(
        ".avatar.big",
        &[("width", "84px"), ("height", "84px"), ("font-size", "30px")],
    );
    s.rule(".muted-icon", &[("font-size", "12px")]);
    s.rule(
        ".call-panel",
        &[
            ("display", "flex"),
            ("align-items", "center"),
            ("gap", "4px"),
            ("margin", "0 8px 8px"),
            ("padding", "8px"),
            (
                "background",
                "color-mix(in srgb, var(--accent-2) 14%, var(--side))",
            ),
            (
                "border",
                "1px solid color-mix(in srgb, var(--accent-2) 40%, transparent)",
            ),
            ("border-radius", "12px"),
        ],
    );
    s.rule(
        ".call-text",
        &[
            ("flex", "1"),
            ("min-width", "0"),
            ("display", "grid"),
            ("text-align", "left"),
            ("padding", "0 4px"),
        ],
    );
    s.rule(
        ".call-text strong",
        &[("color", "var(--accent-2)"), ("font-size", "14px")],
    );
    s.rule(
        ".call-text small",
        &[
            ("color", "var(--muted)"),
            ("overflow", "hidden"),
            ("text-overflow", "ellipsis"),
            ("white-space", "nowrap"),
            ("font-size", "12px"),
        ],
    );
    s.rule(".icon-btn.off", &[("color", "var(--danger)")]);
    s.rule(
        ".icon-btn.hangup, .call-controls .hangup",
        &[("color", "#fff"), ("background", "var(--danger)")],
    );
    s.rule(
        ".icon-btn.hangup:hover",
        &[
            ("background", "var(--danger)"),
            ("filter", "brightness(1.1)"),
        ],
    );
    s.rule(
        ".guild .in-call",
        &[
            ("position", "absolute"),
            ("left", "-4px"),
            ("bottom", "-4px"),
            ("font-size", "12px"),
            ("background", "var(--rail)"),
            ("border-radius", "50%"),
            ("padding", "1px"),
        ],
    );
    s.rule(
        ".voice",
        &[
            ("flex", "1"),
            ("min-height", "0"),
            ("display", "flex"),
            ("flex-direction", "column"),
            ("padding", "16px 16px calc(16px + var(--safe-bottom))"),
            ("gap", "16px"),
            ("overflow-y", "auto"),
        ],
    );
    s.rule(
        ".tiles",
        &[
            ("flex", "1"),
            ("display", "grid"),
            ("gap", "12px"),
            (
                "grid-template-columns",
                "repeat(auto-fit, minmax(min(100%, 260px), 1fr))",
            ),
            ("align-content", "center"),
        ],
    );
    s.rule(
        ".tiles.many",
        &[(
            "grid-template-columns",
            "repeat(auto-fit, minmax(min(100%, 180px), 1fr))",
        )],
    );
    s.rule(
        ".tile",
        &[
            ("position", "relative"),
            ("aspect-ratio", "16 / 10"),
            ("border-radius", "16px"),
            ("background", "var(--rail)"),
            ("display", "grid"),
            ("place-items", "center"),
            ("overflow", "hidden"),
            ("border", "2px solid transparent"),
            ("transition", "border-color 0.15s"),
        ],
    );
    s.rule(".tile.speaking", &[("border-color", "var(--accent-2)")]);
    s.rule(
        ".tile video",
        &[
            ("position", "absolute"),
            ("inset", "0"),
            ("width", "100%"),
            ("height", "100%"),
            ("object-fit", "cover"),
            ("background", "#000"),
        ],
    );
    s.rule(".tile video.mirror", &[("transform", "scaleX(-1)")]);
    s.rule(
        ".tile-name",
        &[
            ("position", "absolute"),
            ("left", "10px"),
            ("bottom", "10px"),
            ("max-width", "calc(100% - 20px)"),
            ("padding", "3px 10px"),
            ("border-radius", "8px"),
            ("background", "#000a"),
            ("color", "#fff"),
            ("font-size", "13px"),
            ("overflow", "hidden"),
            ("text-overflow", "ellipsis"),
            ("white-space", "nowrap"),
        ],
    );
    s.rule(
        ".call-controls",
        &[
            ("display", "flex"),
            ("justify-content", "center"),
            ("flex-wrap", "wrap"),
            ("gap", "10px"),
        ],
    );
    s.rule(
        ".call-controls button",
        &[
            ("display", "grid"),
            ("justify-items", "center"),
            ("gap", "2px"),
            ("min-width", "76px"),
            ("padding", "10px 12px"),
            ("border-radius", "14px"),
            ("background", "var(--elev)"),
            ("border", "1px solid var(--line)"),
            ("font-size", "20px"),
        ],
    );
    s.rule(
        ".call-controls button span",
        &[("font-size", "11px"), ("color", "var(--muted)")],
    );
    s.rule(
        ".call-controls button.on",
        &[("background", "var(--accent)"), ("color", "#fff")],
    );
    s.rule(
        ".call-controls button.on span, .call-controls .hangup span",
        &[("color", "#fffc")],
    );
    s.rule(".call-controls button.off", &[("color", "var(--danger)")]);
    s.rule(
        ".call-join",
        &[
            ("display", "grid"),
            ("justify-items", "center"),
            ("gap", "10px"),
            ("text-align", "center"),
            ("padding-bottom", "12px"),
        ],
    );
    s.rule(".call-join p", &[("margin", "0"), ("max-width", "52ch")]);
    s.rule(
        ".call-join .primary, .button-link",
        &[
            ("min-height", "48px"),
            ("padding", "0 26px"),
            ("border-radius", "12px"),
            ("background", "var(--accent)"),
            ("color", "#fff"),
            ("font-weight", "700"),
            ("display", "inline-grid"),
            ("place-items", "center"),
            ("text-decoration", "none"),
        ],
    );
    // ---- Messages : vocaux, signatures, historique -----------------------
    s.rule(
        ".voice-note",
        &[
            ("display", "block"),
            ("margin-top", "4px"),
            ("width", "min(320px, 100%)"),
            ("height", "40px"),
        ],
    );
    s.rule(
        ".mic.recording",
        &[
            ("color", "var(--danger)"),
            ("font-size", "14px"),
            ("font-variant-numeric", "tabular-nums"),
            ("min-width", "64px"),
        ],
    );
    s.rule(
        ".message.forged",
        &[(
            "background",
            "color-mix(in srgb, var(--danger) 10%, transparent)",
        )],
    );
    s.rule(
        ".warn",
        &[
            ("color", "var(--danger)"),
            ("font-size", "12px"),
            ("font-weight", "700"),
        ],
    );
    s.rule(
        ".unverified",
        &[
            ("color", "var(--faint)"),
            ("font-size", "11px"),
            ("border", "1px solid var(--line)"),
            ("border-radius", "50%"),
            ("width", "16px"),
            ("height", "16px"),
            ("display", "inline-grid"),
            ("place-items", "center"),
            ("cursor", "help"),
        ],
    );
    s.rule(
        ".load-more",
        &[
            ("display", "block"),
            ("margin", "12px auto"),
            ("padding", "6px 14px"),
            ("border-radius", "999px"),
            ("background", "var(--elev)"),
            ("color", "var(--muted)"),
            ("font-size", "13px"),
        ],
    );
    s.rule(
        ".kick",
        &[
            ("opacity", "0"),
            ("margin-left", "auto"),
            ("color", "var(--danger)"),
        ],
    );
    s.rule(
        ".member:hover .kick, .member:focus-within .kick",
        &[("opacity", "1")],
    );
    s.media("(pointer: coarse)", |s| {
        s.rule(".kick", &[("opacity", "0.7")]);
    });
    s.rule(
        ".modal button.danger-fill",
        &[
            ("background", "var(--danger)"),
            ("color", "#fff"),
            ("border-color", "transparent"),
        ],
    );
    // ---- Salons éphémères : le hall ----------------------------------------
    s.rule(
        ".notice",
        &[
            ("width", "100%"),
            ("display", "grid"),
            ("gap", "8px"),
            ("text-align", "left"),
            ("padding", "14px"),
            ("border-radius", "14px"),
            (
                "background",
                "color-mix(in srgb, var(--accent) 12%, var(--chat))",
            ),
            (
                "border",
                "1px solid color-mix(in srgb, var(--accent) 35%, transparent)",
            ),
        ],
    );
    s.rule(".notice p", &[("margin", "0"), ("color", "var(--muted)")]);
    s.rule(
        ".notice input",
        &[
            ("width", "100%"),
            ("min-height", "44px"),
            ("padding", "10px 12px"),
            ("font-size", "16px"),
            ("border-radius", "10px"),
            ("border", "1px solid var(--line)"),
            ("background", "var(--rail)"),
            ("color", "var(--fg)"),
        ],
    );
    s.rule(
        ".lobby",
        &[
            ("min-height", "100%"),
            ("display", "flex"),
            ("flex-direction", "column"),
            ("overflow-y", "auto"),
            ("height", "var(--app-height)"),
        ],
    );
    s.rule(
        ".lobby-head",
        &[
            ("position", "sticky"),
            ("top", "0"),
            ("z-index", "2"),
            ("display", "flex"),
            ("align-items", "center"),
            ("gap", "12px"),
            ("padding", "calc(12px + var(--safe-top)) calc(16px + var(--safe-right)) 12px calc(16px + var(--safe-left))"),
            ("background", "color-mix(in srgb, var(--bg) 85%, transparent)"),
            ("backdrop-filter", "blur(10px)"),
            ("-webkit-backdrop-filter", "blur(10px)"),
            ("border-bottom", "1px solid var(--line)"),
        ],
    );
    s.rule(".lobby-head .spacer, .chat-head .spacer", &[("flex", "1")]);
    s.rule(
        ".brand",
        &[
            ("display", "inline-flex"),
            ("align-items", "center"),
            ("gap", "10px"),
            ("color", "var(--fg)"),
            ("text-decoration", "none"),
            ("font-size", "18px"),
        ],
    );
    s.rule(
        ".me-chip",
        &[
            ("display", "inline-flex"),
            ("align-items", "center"),
            ("gap", "8px"),
            ("padding", "4px 12px 4px 4px"),
            ("border-radius", "999px"),
            ("background", "var(--elev)"),
            ("border", "1px solid var(--line)"),
            ("max-width", "50vw"),
        ],
    );
    s.rule(
        ".me-chip span:last-child",
        &[
            ("overflow", "hidden"),
            ("text-overflow", "ellipsis"),
            ("white-space", "nowrap"),
        ],
    );
    s.rule(
        ".lobby-main",
        &[
            ("width", "min(720px, 100%)"),
            ("margin", "0 auto"),
            ("padding", "24px 16px calc(32px + var(--safe-bottom))"),
            ("display", "grid"),
            ("gap", "14px"),
        ],
    );
    s.rule(
        ".lobby-main h1",
        &[
            ("margin", "8px 0 0"),
            ("font-size", "clamp(26px, 5vw, 34px)"),
        ],
    );
    s.rule(".lobby-main > p", &[("margin", "0")]);
    s.rule(".search", &[("display", "flex"), ("gap", "8px")]);
    s.rule(
        ".search input",
        &[
            ("flex", "1"),
            ("min-width", "0"),
            ("min-height", "50px"),
            ("padding", "0 16px"),
            ("font-size", "17px"),
            ("border-radius", "14px"),
            ("border", "1px solid var(--line)"),
            ("background", "var(--elev)"),
            ("color", "var(--fg)"),
        ],
    );
    s.rule(
        ".search button",
        &[
            ("min-height", "50px"),
            ("padding", "0 20px"),
            ("border-radius", "14px"),
            ("background", "var(--accent)"),
            ("color", "var(--accent-fg)"),
            ("font-weight", "700"),
        ],
    );
    s.rule(
        ".rooms",
        &[("display", "grid"), ("gap", "10px"), ("margin-top", "6px")],
    );
    s.rule(
        ".room-card",
        &[
            ("display", "grid"),
            ("gap", "2px"),
            ("text-align", "left"),
            ("padding", "14px 16px"),
            ("border-radius", "14px"),
            ("background", "var(--side)"),
            ("border", "1px solid var(--line)"),
            ("transition", "border-color .15s, transform .15s"),
        ],
    );
    s.rule(
        ".room-card:hover",
        &[
            ("border-color", "var(--accent)"),
            ("transform", "translateY(-1px)"),
        ],
    );
    s.rule(
        ".room-card.create",
        &[
            ("border-style", "dashed"),
            ("border-color", "var(--accent)"),
        ],
    );
    s.rule(
        ".room-name",
        &[
            ("font-weight", "700"),
            ("font-size", "17px"),
            ("overflow-wrap", "anywhere"),
        ],
    );
    s.rule(
        ".room-meta",
        &[("color", "var(--muted)"), ("font-size", "13px")],
    );
    s.rule(
        ".empty",
        &[
            ("color", "var(--muted)"),
            ("text-align", "center"),
            ("padding", "24px 8px"),
            ("margin", "0"),
        ],
    );
    s.rule(".empty.error", &[("color", "var(--danger)")]);
    // ---- Connexion, langue, blocage --------------------------------------------
    s.rule(
        ".card-top",
        &[
            ("display", "flex"),
            ("width", "100%"),
            ("margin-bottom", "-6px"),
        ],
    );
    s.rule(
        "select.lang",
        &[
            ("min-height", "36px"),
            ("padding", "0 10px"),
            ("border-radius", "10px"),
            ("border", "1px solid var(--line)"),
            ("background", "var(--elev)"),
            ("color", "var(--fg)"),
            ("font", "inherit"),
            ("font-size", "14px"),
        ],
    );
    s.rule(
        ".welcome-card button.link-button",
        &[
            ("background", "transparent"),
            ("border-color", "transparent"),
            ("color", "var(--accent)"),
            ("text-decoration", "underline"),
            ("text-underline-offset", "3px"),
        ],
    );
    s.rule(
        ".welcome-card .field input:invalid",
        &[("box-shadow", "none")],
    );
    s.rule(".welcome-card.banned", &[("border-color", "var(--danger)")]);
    s.rule(".big-icon", &[("font-size", "56px"), ("line-height", "1")]);
    s.rule(
        ".mine",
        &[
            ("display", "grid"),
            ("gap", "4px"),
            ("padding", "14px 16px"),
            ("border-radius", "16px"),
            ("border", "1px solid var(--line)"),
            ("background", "var(--side)"),
        ],
    );
    s.rule(".mine h2", &[("margin", "0"), ("font-size", "18px")]);
    s.rule(".mine .room-card", &[("background", "var(--elev)")]);
    s.rule(
        ".mine .empty",
        &[("padding", "8px 0"), ("text-align", "left")],
    );
    s.rule(".mine[hidden], .mine [hidden]", &[("display", "none")]);
    // Les abonnements.
    s.rule(
        ".plans",
        &[
            ("display", "grid"),
            ("gap", "8px"),
            ("margin-top", "28px"),
            ("padding", "16px"),
            ("border-radius", "16px"),
            ("border", "1px solid var(--accent)"),
            ("background", "var(--side)"),
        ],
    );
    s.rule(".plans[hidden]", &[("display", "none")]);
    s.rule(".plans h2", &[("margin", "0"), ("font-size", "18px")]);
    s.rule(".plans p", &[("margin", "0")]);
    s.rule(
        ".plan-cards",
        &[
            ("display", "grid"),
            (
                "grid-template-columns",
                "repeat(auto-fit, minmax(180px, 1fr))",
            ),
            ("gap", "10px"),
            ("margin-top", "4px"),
        ],
    );
    s.rule(
        ".plan-card",
        &[
            ("display", "grid"),
            ("gap", "6px"),
            ("padding", "14px"),
            ("border-radius", "14px"),
            ("border", "1px solid var(--line)"),
            ("background", "var(--elev)"),
        ],
    );
    s.rule(".plan-card strong", &[("font-size", "17px")]);
    s.rule(".plan-card .price", &[("color", "var(--muted)")]);
    s.rule(
        ".plan-card button, #manage",
        &[
            ("min-height", "44px"),
            ("padding", "0 18px"),
            ("border-radius", "12px"),
            ("background", "var(--accent)"),
            ("color", "var(--accent-fg)"),
            ("font-weight", "700"),
        ],
    );
    s.rule(
        ".plan-card button:disabled",
        &[("opacity", ".5"), ("cursor", "not-allowed")],
    );
    s.rule(".subscribed", &[("font-weight", "600")]);
    s.rule("#manage", &[("justify-self", "start")]);
    s.rule(
        ".section-head",
        &[
            ("display", "flex"),
            ("align-items", "center"),
            ("justify-content", "space-between"),
            ("gap", "12px"),
        ],
    );
    s.rule(".section-head .link-button", &[("font-size", "13px")]);
    // Un salon dont se souvient l'appareil : la carte, et de quoi l'oublier.
    s.rule(".recent-card", &[("position", "relative")]);
    s.rule(".recent-card .room-card", &[("padding-right", "48px")]);
    s.rule(
        "button.forget",
        &[
            ("position", "absolute"),
            ("top", "8px"),
            ("right", "8px"),
            ("width", "36px"),
            ("height", "36px"),
            ("border-radius", "10px"),
            ("color", "var(--muted)"),
            ("background", "transparent"),
        ],
    );
    s.rule(
        "button.forget:hover, button.forget:focus-visible",
        &[("background", "var(--hover)"), ("color", "var(--fg)")],
    );
    s.rule(
        ".created-badge",
        &[
            ("justify-self", "start"),
            ("margin-top", "4px"),
            ("padding", "2px 8px"),
            ("border-radius", "999px"),
            ("font-size", "12px"),
            ("color", "var(--accent)"),
            ("border", "1px solid var(--accent)"),
        ],
    );
    s.rule(".key-badge", &[("font-size", "12px")]);
    // L'administration du salon.
    s.rule(".member .name", &[("flex", "1"), ("min-width", "0")]);
    s.rule(
        "button.expel",
        &[
            ("margin-left", "auto"),
            ("padding", "4px 10px"),
            ("border-radius", "8px"),
            ("border", "1px solid var(--line)"),
            ("background", "transparent"),
            ("color", "var(--danger)"),
            ("font-size", "12px"),
            ("font-weight", "600"),
        ],
    );
    // ---- Le salon ------------------------------------------------------------
    s.rule(
        ".room-layout, .room-layout.members-open",
        &[("grid-template-columns", "minmax(0, 1fr) 0")],
    );
    s.rule(
        ".room-layout.members-open",
        &[(
            "grid-template-columns",
            "minmax(0, 1fr) calc(260px + var(--safe-right))",
        )],
    );
    s.rule(
        ".room-layout .chat",
        &[("padding-left", "var(--safe-left)")],
    );
    s.rule(".chat-head .topic", &[("display", "inline")]);
    s.rule(
        "#call-btn.on",
        &[("background", "var(--danger)"), ("color", "#fff")],
    );
    s.rule(
        ".call-invite",
        &[
            ("display", "flex"),
            ("align-items", "center"),
            ("justify-content", "space-between"),
            ("gap", "10px"),
            ("margin", "8px 16px 0"),
            ("padding", "8px 8px 8px 14px"),
            ("border-radius", "12px"),
            (
                "background",
                "color-mix(in srgb, var(--accent-2) 14%, var(--chat))",
            ),
            (
                "border",
                "1px solid color-mix(in srgb, var(--accent-2) 40%, transparent)",
            ),
        ],
    );
    s.rule(
        ".call-invite button",
        &[
            ("min-height", "38px"),
            ("padding", "0 16px"),
            ("border-radius", "10px"),
            ("background", "var(--accent)"),
            ("color", "#fff"),
            ("font-weight", "700"),
        ],
    );
    s.rule(
        ".call-area",
        &[
            ("display", "grid"),
            ("gap", "10px"),
            ("padding", "10px 16px"),
            ("border-bottom", "1px solid var(--line)"),
            ("max-height", "52vh"),
            ("overflow-y", "auto"),
        ],
    );
    s.rule(
        ".call-area .tiles",
        &[(
            "grid-template-columns",
            "repeat(auto-fit, minmax(min(100%, 200px), 1fr))",
        )],
    );
    s.rule(".call-area .tile", &[("aspect-ratio", "16 / 9")]);
    s.rule(
        ".call-area .avatar.big",
        &[("width", "64px"), ("height", "64px"), ("font-size", "24px")],
    );
    s.rule(
        ".member.speaking .avatar",
        &[("box-shadow", "0 0 0 2px var(--accent-2)")],
    );
    s.media("(max-width: 860px)", |s| {
        s.rule(".room-layout .members", &[("width", "min(300px, 85vw)")]);
        s.rule(".chat-head .topic", &[("display", "none")]);
        s.rule(".call-area", &[("max-height", "45vh")]);
    });
    // Le hall ne déborde jamais, même sur un petit écran.
    s.rule(".lobby", &[("overflow-x", "hidden")]);
    s.rule(
        ".lobby-main",
        &[
            ("grid-template-columns", "minmax(0, 1fr)"),
            ("box-sizing", "border-box"),
        ],
    );
    s.rule(".lobby-main > *", &[("min-width", "0")]);
    s.rule(".room-card", &[("min-width", "0"), ("width", "100%")]);
    s.rule(".check.small", &[("align-items", "flex-start")]);
    s.rule(
        ".check.small input",
        &[("flex", "none"), ("margin-top", "2px")],
    );
    s.media("(max-width: 860px)", |s| {
        s.rule(
            ".call-area .tiles",
            &[(
                "grid-template-columns",
                "repeat(auto-fit, minmax(min(100%, 140px), 1fr))",
            )],
        );
        s.rule(".call-area .tile", &[("aspect-ratio", "4 / 3")]);
        s.rule(
            ".call-area .avatar.big",
            &[("width", "52px"), ("height", "52px"), ("font-size", "20px")],
        );
        s.rule(
            ".call-controls button",
            &[("min-width", "64px"), ("padding", "8px")],
        );
    });
    s.css()
}
