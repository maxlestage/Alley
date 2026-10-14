//! Construire le DOM depuis Rust, sans HTML : `h("div.card#x").text("…")`.
//!
//! Les écouteurs d'événements sont rangés par **zone** (`scope("messages",
//! || …)`) : quand une zone est redessinée, les écouteurs de son ancien rendu
//! sont libérés avec lui. Rien ne s'accumule, même après des milliers de
//! messages.

use std::cell::RefCell;
use std::collections::HashMap;
use std::future::Future;

use js_sys::{Array, Function, Promise, Reflect};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Document, Element, Event, HtmlElement, HtmlInputElement, Window};

type Listener = Closure<dyn FnMut(Event)>;

thread_local! {
    static SCOPES: RefCell<HashMap<String, Vec<Listener>>> = RefCell::new(HashMap::new());
    static STACK: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

pub fn window() -> Window {
    web_sys::window().expect("fenêtre")
}

pub fn document() -> Document {
    window().document().expect("document")
}

/// Exécute `f` dans la zone `name` : les écouteurs créés pendant `f`
/// remplacent ceux du rendu précédent de cette zone.
pub fn scope<T>(name: &str, f: impl FnOnce() -> T) -> T {
    let old = SCOPES.with(|s| s.borrow_mut().remove(name));
    STACK.with(|s| s.borrow_mut().push(name.to_owned()));
    let out = f();
    STACK.with(|s| s.borrow_mut().pop());
    // Libérés après coup : un écouteur peut être en train de s'exécuter.
    drop(old);
    out
}

fn keep(listener: Listener) {
    let name = STACK
        .with(|s| s.borrow().last().cloned())
        .unwrap_or_else(|| "global".to_owned());
    SCOPES.with(|s| s.borrow_mut().entry(name).or_default().push(listener));
}

/// Écoute un événement sur n'importe quelle cible, dans la zone courante.
pub fn listen(target: &web_sys::EventTarget, event: &str, f: impl FnMut(Event) + 'static) {
    let closure = Closure::<dyn FnMut(Event)>::new(f);
    let _ = target.add_event_listener_with_callback(event, closure.as_ref().unchecked_ref());
    keep(closure);
}

/// Un écouteur permanent (fenêtre, document) : jamais libéré.
pub fn listen_forever(target: &web_sys::EventTarget, event: &str, f: impl FnMut(Event) + 'static) {
    let closure = Closure::<dyn FnMut(Event)>::new(f);
    let _ = target.add_event_listener_with_callback(event, closure.as_ref().unchecked_ref());
    closure.forget();
}

/// Un élément en construction.
pub struct El(pub Element);

/// Ce qu'on peut mettre dans un élément.
pub enum Child {
    El(Element),
    Text(String),
    None,
}

impl From<El> for Child {
    fn from(e: El) -> Self {
        Child::El(e.0)
    }
}
impl From<Element> for Child {
    fn from(e: Element) -> Self {
        Child::El(e)
    }
}
impl From<&str> for Child {
    fn from(t: &str) -> Self {
        Child::Text(t.to_owned())
    }
}
impl From<String> for Child {
    fn from(t: String) -> Self {
        Child::Text(t)
    }
}
impl From<&String> for Child {
    fn from(t: &String) -> Self {
        Child::Text(t.clone())
    }
}
impl<T: Into<Child>> From<Option<T>> for Child {
    fn from(o: Option<T>) -> Self {
        o.map_or(Child::None, Into::into)
    }
}

/// `h("button.primary#ok")` : la balise, puis ses classes et son identifiant.
pub fn h(spec: &str) -> El {
    let end = spec.find(['.', '#']).unwrap_or(spec.len());
    let tag = if end == 0 { "div" } else { &spec[..end] };
    let el = document().create_element(tag).expect("élément");
    let mut rest = &spec[end..];
    while !rest.is_empty() {
        let kind = &rest[..1];
        let next = rest[1..].find(['.', '#']).map_or(rest.len(), |i| i + 1);
        let name = &rest[1..next];
        if kind == "." {
            let _ = el.class_list().add_1(name);
        } else {
            el.set_id(name);
        }
        rest = &rest[next..];
    }
    El(el)
}

impl El {
    pub fn class(self, c: &str) -> Self {
        for name in c.split_whitespace() {
            let _ = self.0.class_list().add_1(name);
        }
        self
    }

    pub fn class_if(self, c: &str, on: bool) -> Self {
        if on { self.class(c) } else { self }
    }

    pub fn id(self, id: &str) -> Self {
        self.0.set_id(id);
        self
    }

    pub fn attr(self, k: &str, v: impl AsRef<str>) -> Self {
        let _ = self.0.set_attribute(k, v.as_ref());
        self
    }

    /// Une propriété JavaScript (`value`, `checked`, `muted`…).
    pub fn prop(self, k: &str, v: impl Into<JsValue>) -> Self {
        let _ = Reflect::set(&self.0, &JsValue::from_str(k), &v.into());
        self
    }

    pub fn data(self, k: &str, v: impl AsRef<str>) -> Self {
        self.attr(&format!("data-{k}"), v)
    }

    /// Une variable CSS (`--h`) ou une propriété de style.
    pub fn style(self, prop: &str, value: impl AsRef<str>) -> Self {
        if let Some(e) = self.0.dyn_ref::<HtmlElement>() {
            let _ = e.style().set_property(prop, value.as_ref());
        }
        self
    }

    pub fn text(self, t: impl AsRef<str>) -> Self {
        let _ = self.0.append_with_str_1(t.as_ref());
        self
    }

    pub fn child(self, c: impl Into<Child>) -> Self {
        append(&self.0, c.into());
        self
    }

    pub fn children<C: Into<Child>>(self, items: impl IntoIterator<Item = C>) -> Self {
        for c in items {
            append(&self.0, c.into());
        }
        self
    }

    pub fn on(self, event: &str, f: impl FnMut(Event) + 'static) -> Self {
        listen(&self.0, event, f);
        self
    }

    /// Un clic qui lance une tâche asynchrone.
    pub fn click<F, Fut>(self, f: F) -> Self
    where
        F: Fn() -> Fut + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        self.on("click", move |e| {
            e.prevent_default();
            e.stop_propagation();
            spawn(f());
        })
    }

    pub fn el(self) -> Element {
        self.0
    }
}

pub fn append(parent: &Element, c: Child) {
    match c {
        Child::El(e) => {
            let _ = parent.append_child(&e);
        }
        Child::Text(t) => {
            let _ = parent.append_with_str_1(&t);
        }
        Child::None => {}
    }
}

/// Remplace les enfants d'un élément.
pub fn replace(parent: &Element, children: Vec<Child>) {
    parent.set_text_content(None);
    for c in children {
        append(parent, c);
    }
}

pub fn by_id(id: &str) -> Option<Element> {
    document().get_element_by_id(id)
}

pub fn query(selector: &str) -> Option<Element> {
    document().query_selector(selector).ok().flatten()
}

pub fn query_all(selector: &str) -> Vec<Element> {
    let Ok(list) = document().query_selector_all(selector) else {
        return Vec::new();
    };
    (0..list.length())
        .filter_map(|i| list.item(i)?.dyn_into::<Element>().ok())
        .collect()
}

pub fn input_value(el: &Element) -> String {
    if let Some(i) = el.dyn_ref::<HtmlInputElement>() {
        i.value()
    } else if let Some(t) = el.dyn_ref::<web_sys::HtmlTextAreaElement>() {
        t.value()
    } else {
        // Une liste déroulante (`<select>`).
        get(el, "value").as_string().unwrap_or_default()
    }
}

pub fn set_input_value(el: &Element, v: &str) {
    if let Some(i) = el.dyn_ref::<HtmlInputElement>() {
        i.set_value(v);
    } else if let Some(t) = el.dyn_ref::<web_sys::HtmlTextAreaElement>() {
        t.set_value(v);
    }
}

pub fn focus(el: &Element) {
    if let Some(e) = el.dyn_ref::<HtmlElement>() {
        let _ = e.focus();
    }
}

pub fn toggle_class(el: &Element, class: &str, on: bool) {
    let _ = el.class_list().toggle_with_force(class, on);
}

/// Le contenu d'une balise `<meta name="…">`.
pub fn meta(name: &str) -> String {
    query(&format!("meta[name=\"{name}\"]"))
        .and_then(|m| m.get_attribute("content"))
        .unwrap_or_default()
}

pub fn spawn(f: impl Future<Output = ()> + 'static) {
    wasm_bindgen_futures::spawn_local(f);
}

pub async fn sleep(ms: i32) {
    let promise = Promise::new(&mut |resolve, _| {
        let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
    });
    let _ = JsFuture::from(promise).await;
}

pub fn now() -> f64 {
    js_sys::Date::now()
}

pub fn get(obj: &JsValue, key: &str) -> JsValue {
    Reflect::get(obj, &JsValue::from_str(key)).unwrap_or(JsValue::UNDEFINED)
}

/// Appelle une méthode par son nom (pour les API que web-sys n'expose pas :
/// `navigator.share`, `setAppBadge`, `clipboard`…).
pub fn call(obj: &JsValue, method: &str, args: &[JsValue]) -> Result<JsValue, JsValue> {
    let f: Function = get(obj, method).dyn_into()?;
    f.apply(obj, &args.iter().cloned().collect::<Array>())
}

pub async fn await_promise(v: Result<JsValue, JsValue>) -> Result<JsValue, JsValue> {
    let p: Promise = v?.dyn_into()?;
    JsFuture::from(p).await
}

pub fn has(obj: &JsValue, key: &str) -> bool {
    !get(obj, key).is_undefined()
}

pub fn matches_media(query: &str) -> bool {
    window()
        .match_media(query)
        .ok()
        .flatten()
        .is_some_and(|m| m.matches())
}

/// Un petit message en bas de l'écran.
pub fn toast(text: impl AsRef<str>, error: bool) {
    let Some(box_) = by_id("toasts") else { return };
    let el = h("div.toast")
        .class_if("error", error)
        .attr("role", "status")
        .text(text)
        .el();
    let _ = box_.append_child(&el);
    spawn(async move {
        sleep(3600).await;
        let _ = el.class_list().add_1("out");
        sleep(400).await;
        el.remove();
    });
}

pub fn confirm(text: &str) -> bool {
    window().confirm_with_message(text).unwrap_or(false)
}

/// Copie dans le presse-papiers, ou affiche le texte à copier.
pub async fn copy(text: &str, done: &str) {
    let clipboard = get(&window().navigator(), "clipboard");
    let ok = !clipboard.is_undefined()
        && await_promise(call(&clipboard, "writeText", &[JsValue::from_str(text)]))
            .await
            .is_ok();
    if ok {
        toast(done, false);
    } else {
        let _ = window().prompt_with_message_and_default(crate::i18n::t().copy_prompt, text);
    }
}

// ---------------------------------------------------------------------------
// Dates.

fn date(ts: f64) -> js_sys::Date {
    js_sys::Date::new(&JsValue::from_f64(ts))
}

/// `14:05`.
pub fn time_label(ts: f64) -> String {
    let d = date(ts);
    format!("{:02}:{:02}", d.get_hours(), d.get_minutes())
}

thread_local! {
    static IGNORE: Closure<dyn FnMut(JsValue)> = Closure::new(|_| {});
}

/// Lance la lecture d'un média ; une lecture interrompue (média remplacé ou
/// retiré) n'est pas une erreur.
pub fn play(media: &web_sys::HtmlMediaElement) {
    if let Ok(p) = media.play() {
        IGNORE.with(|f| {
            let _ = p.catch(f);
        });
    }
}
