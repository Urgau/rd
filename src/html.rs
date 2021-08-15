use crate::markdown;
use crate::pp;
use anyhow::{Context as _, Result};
use rustdoc_types::*;
use std::borrow::Cow;
use std::fs::{DirBuilder, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use tracing::{debug, error, info, trace, warn};
use typed_arena::Arena;

pub(crate) struct GlobalContext<'krate> {
    pub(crate) krate: &'krate Crate,
    pub(crate) krate_name: &'krate str,
    pub(crate) files: Arena<PathBuf>,
    pub(crate) item_paths: Arena<ItemPath>,
    pub(crate) output_dir: &'krate PathBuf,
}

pub(crate) struct PageContext<'context> {
    #[allow(dead_code)]
    item: &'context Item,
    filepath: &'context PathBuf,
    filename: PathBuf,
    item_path: &'context ItemPath,
}

pub struct ItemPath(pub(crate) Vec<ItemPathComponent>);

#[derive(Clone)]
pub struct ItemPathComponent {
    pub(crate) name: String,
    pub(crate) kind: &'static str,
    pub(crate) filepath: PathBuf,
}

impl<'context> ItemPath {
    fn display(
        &'context self,
        page_context: &'context PageContext<'context>,
    ) -> ItemPathDisplay<'context> {
        ItemPathDisplay(self, page_context)
    }
}

pub struct ItemPathDisplay<'a>(&'a ItemPath, &'a PageContext<'a>);

impl<'context> markup::Render for ItemPathDisplay<'context> {
    fn render(&self, writer: &mut impl std::fmt::Write) -> std::fmt::Result {
        for (index, item_path_component) in self.0 .0.iter().enumerate() {
            if index != 0 {
                writer.write_str("::")?;
            }
            writer.write_str("<a class=\"")?;
            writer.write_str(item_path_component.kind)?;

            writer.write_str("\" href=\"")?;
            let href = relative(self.1.filepath, &item_path_component.filepath);
            writer.write_fmt(format_args!("{}", href.display()))?;

            writer.write_str("\">")?;
            writer.write_str(&item_path_component.name)?;

            writer.write_str("</a>")?;
        }
        Ok(())
    }
}

const STYLE_CSS: &'static str = "style.css";
const RUST_SVG: &'static str = "rust.svg";
const SEARCH_JS: &'static str = "search.js";
const SEARCH_INDEX_JS: &'static str = "search-index.js";

enum TocDestination<'a> {
    Id(String),
    File(&'a PathBuf),
}

impl<'a> markup::Render for TocDestination<'a> {
    fn render(&self, writer: &mut impl std::fmt::Write) -> std::fmt::Result {
        match self {
            TocDestination::Id(id) => {
                writer.write_char('#')?;
                writer.write_str(id)
            }
            TocDestination::File(path) => {
                write!(writer, "{}", path.display())
            }
        }
    }
}

pub struct TocSection<'toc> {
    name: &'static str,
    id: &'static str,
    items: Vec<(Cow<'toc, str>, TocDestination<'toc>)>,
}

pub struct BodyInformations<'a> {
    page_title: String,
    krate_name: &'a str,
    root_path: PathBuf,
}

impl<'context, 'krate> BodyInformations<'krate> {
    fn with(
        global_context: &'context GlobalContext<'krate>,
        page_context: &'context PageContext<'context>,
    ) -> Self {
        let mut page_title = String::with_capacity(32);

        for (index, path) in page_context.item_path.0.iter().enumerate() {
            if index + 1 == page_context.item_path.0.len() {
                page_title.insert_str(0, &path.name);
            } else {
                if index == 0 {
                    page_title.push_str(" in ");
                } else {
                    page_title.push_str("::");
                }
                page_title.push_str(&path.name);
            }
        }
        page_title.push_str(" - Rust");

        Self {
            page_title,
            krate_name: global_context.krate_name,
            root_path: top_of(&page_context.filepath),
        }
    }
}

const ASSOCIATED_TYPES: &str = "Associated Types";
const ASSOCIATED_TYPES_ID: &str = "associated-types";
const ASSOCIATED_CONSTS: &str = "Associated Consts";
const ASSOCIATED_CONSTS_ID: &str = "associated-consts";
const REQUIRED_METHODS: &str = "Required Methods";
const REQUIRED_METHODS_ID: &str = "required-methods";
const PROVIDED_METHODS: &str = "Provided Methods";
const PROVIDED_METHODS_ID: &str = "provided-methods";
const IMPLEMENTATION_FOREIGN_TYPES: &str = "Implementations on Foreign Types";
const IMPLEMENTATION_FOREIGN_TYPES_ID: &str = "implementations-foreign-types";
const IMPLEMENTORS: &str = "Implementors";
const IMPLEMENTORS_ID: &str = "implementors";
const AUTO_IMPLEMENTORS: &str = "Auto Implementors";
const AUTO_IMPLEMENTORS_ID: &str = "auto-implementors";
const IMPLEMENTATIONS: &str = "Implementations";
const IMPLEMENTATIONS_ID: &str = "implementations";
const TRAIT_IMPLEMENTATIONS: &str = "Trait Implementations";
const TRAIT_IMPLEMENTATIONS_ID: &str = "trait-implementations";
const AUTO_TRAIT_IMPLEMENTATIONS: &str = "Auto Trait Implementations";
const AUTO_TRAIT_IMPLEMENTATIONS_ID: &str = "auto-trait-implementations";
const BLANKET_IMPLEMENTATIONS: &str = "Blanket Implementations";
const BLANKET_IMPLEMENTATIONS_ID: &str = "blanket-implementations";

const IMPORTS: &str = "Re-exports";
const IMPORTS_ID: &str = "imports";
const MODULES: &str = "Modules";
const MODULES_ID: &str = "modules";
const UNIONS: &str = "Unions";
const UNIONS_ID: &str = "unions";
const STRUCTS: &str = "Structs";
const STRUCTS_ID: &str = "structs";
const ENUMS: &str = "Enums";
const ENUMS_ID: &str = "enums";
const FUNCTIONS: &str = "Functions";
const FUNCTIONS_ID: &str = "functions";
const TRAITS: &str = "Traits";
const TRAITS_ID: &str = "traits";
const TRAIT_ALIAS: &str = "Trait Alias";
const TRAIT_ALIAS_ID: &str = "trait_alias";
const TYPEDEFS: &str = "Type Definitions";
const TYPEDEFS_ID: &str = "typedefs";
const CONSTANTS: &str = "Constants";
const CONSTANTS_ID: &str = "constants";
const MACROS: &str = "Macros";
const MACROS_ID: &str = "macros";
const PROC_MACROS: &str = "Proc Macros";
const PROC_MACROS_ID: &str = "proc_macros";

fn inner<'c>(cow: &'c Cow<'c, str>) -> &'c str {
    match cow {
        Cow::Borrowed(b) => b,
        Cow::Owned(o) => o,
    }
}

markup::define! {
    Base<'a, Body: markup::Render>(infos: BodyInformations<'a>, main: Body) {
        @markup::doctype()
        html[lang="en"] {
            head {
                title { @infos.page_title }
                meta[charset = "utf-8"];
                meta[name = "viewport", content = "width=device-width, initial-scale=1"];
                link[href="https://cdn.jsdelivr.net/npm/bootstrap@5.0.2/dist/css/bootstrap.min.css", rel="stylesheet", integrity="sha384-EVSTQN3/azprG1Anm3QDgpJLIm9Nao0Yz1ztcQTwFspd3yD65VohhpuuCOmLASjC", crossorigin="anonymous"];
                link[href="https://cdn.jsdelivr.net/npm/bootstrap-icons@1.5.0/font/bootstrap-icons.css", rel="stylesheet", crossorigin="anonymous"];
                link[href=format!("{}/{}", infos.root_path.display(), STYLE_CSS), rel="stylesheet"];
                link[href=format!("{}/{}", infos.root_path.display(), RUST_SVG), rel="icon", type="image/svg+xml"];
            }
            body {
                @Header { krate_name: infos.krate_name, rust: &format!("{}/{}", infos.root_path.display(), RUST_SVG), krate_path: &format!("{}/{}/index.html", infos.root_path.display(), infos.krate_name) }
                @Search { krate_name: infos.krate_name }
                #main[class="container-xxl"] {
                    @main
                }
                @Footer { year: 2021 }
                script[src="https://cdn.jsdelivr.net/npm/bootstrap@5.1.0/dist/js/bootstrap.min.js", integrity="sha384-cn7l7gDp0eyniUwwAZgrzD06kc/tftFf19TOAs2zVinnD/C7E91j9yyk5//jjpt/", crossorigin="anonymous"] {}
                script[src=format!("{}/{}/{}", infos.root_path.display(), infos.krate_name, SEARCH_INDEX_JS)] {}
                script[src=format!("{}/{}", infos.root_path.display(), SEARCH_JS)] {}
            }
        }
    }

    ItemPage<'a, Definition: markup::Render, /*Documentation: markup::Render, */Content: markup::Render>(item_type: &'a str, item_name: &'a str, item_path: ItemPathDisplay<'a>, toc: &'a Vec<TocSection<'a>>, item_definition: Option<Definition>, item_doc: Option<markdown::MarkdownWithToc<'a, 'a, 'a, 'a>>, content: Option<Content>) {
        div[class="rd-main"] {
            div#intro[class="rd-intro"] {
                h1[id="item-title", class="rd-anchor item-title"] {
                    span {
                        @item_type
                    }
                    " "
                    @item_path
                }
                @if item_definition.is_some() {
                    pre[id="item-definition", class="rd-anchor rust"] {
                        @item_definition
                    }
                }
                details[id="item-documentation", class="rd-anchor", open=""] {
                    summary {
                        "Expand description"
                    }
                    div[class = "item-doc"] {
                        @item_doc
                    }
                }
            }
            div[id="rd-docs-nav", class="rd-toc ps-xl-3 collapse"] {
                strong[class="d-block h6 my-2 pb-2 border-bottom"] { "On this page" }
                nav#TableOfContents {
                    ul {
                        li {
                            a[href="#item-title", class="d-inline-flex align-items-center rounded"] { strong { @item_name } }
                        }
                        @if let Some(item_doc) = item_doc {
                            li {
                                @if item_doc.4.borrow_mut().len() == 0 {
                                    a[href="#item-documentation", class="d-inline-flex align-items-center rounded"] { strong { "Documentation" } }
                                } else {
                                    //a[href="#item-documentation", class="d-inline-flex align-items-center rounded"] { strong { "Documentation" } }
                                    //ul {
                                    a[class="rd-btn-toc d-inline-flex align-items-center rounded", href="#item-documentation", "data-bs-toggle"="collapse", "data-bs-target"="#toc-documentation", "aria-expanded"="true", "aria-current"="true"] { strong { "Documentation" } }
                                    ul[id="toc-documentation", class="collapse show"] {
                                        @for (level, ref name, ref destination) in item_doc.4.borrow_mut().into_iter() {
                                            @if *level == 1 {
                                                li {
                                                    a[href=format!("#{}", destination), class="d-inline-flex align-items-center rounded"] {
                                                        @name
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        @for TocSection { name: section_name, id: section_id, items: section_items } in toc.iter() {
                            @if section_items.len() > 0 {
                                li {
                                    //a[href=format!("#{}", section_id), class="d-inline-flex align-items-center rounded"] { strong { @section_name } }
                                    //ul {
                                    a[class="rd-btn-toc d-inline-flex align-items-center rounded", href=format!("#{}", section_id), "data-bs-toggle"="collapse", "data-bs-target"=format!("#toc-{}", section_id), "aria-expanded"="true", "aria-current"="true"] { strong { @section_name } }
                                    ul[id=format!("toc-{}", section_id), class="collapse show"] {
                                        @for (ref name, destination) in section_items {
                                            li {
                                                a[href=destination, class="d-inline-flex align-items-center rounded"] {
                                                    @inner(name)
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div[class="rd-content"] {
                @content
            }
        }
    }

    ModuleSection<
        'name,
        Left: markup::Render,
        Right: markup::Render,
    > (name: &'name str, id: &'name str, items: &'name Vec<(Left, Right)>) {
        @if !items.is_empty() {
            h3[id=id, class="border-bottom pb-1 rd-anchor"] { @name }
            div[class = "item-table"] {
                @for (left, right) in *items {
                    div { @left }
                    div { @right }
                }
            }
        }
    }

    GeneralSection<
        'name,
        Item: markup::Render,
    > (name: &'name str, id: &'name str, items: &'name Vec<Item>) {
        @if !items.is_empty() {
            h3[id=id, class="border-bottom pb-1 mt-2 rd-anchor"] { @name }
            @for item in *items {
                @item
            }
        }
    }

    InlineCodeWithSource<
        'source,
        Code: markup::Render,
    > (code: Code, source_href: &'source Option<String>) {
        @if source_href.is_some() {
            a[class = "float-right", href = source_href] {
                "[src]"
            }
        }
        code[class="inline-code"] { @code }
    }

    CodeEnchanted<
        Code: markup::Render,
        Doc: markup::Render,
    > (code: Code, doc: Option<Doc>, id: Option<String>, open: bool, source_href: Option<String>) {
        div[id=id, class="mt-2 mb-2 rd-anchor"] {
            @if doc.is_some() {
                details[open=open] {
                    summary {
                        @InlineCodeWithSource { code, source_href }
                    }
                    div[class="item-doc"] { @doc }
                }
            } else {
                @InlineCodeWithSource { code, source_href }
            }
        }
    }

    CodeEnchantedWithExtras<
        Code: markup::Render,
        Doc: markup::Render,
        Extra: markup::Render,
    > (code: Code, doc: Option<Doc>, extras: Vec<Extra>, id: Option<String>, open: bool, source_href: Option<String>) {
        div[id=id, class="mt-2 mb-2"] {
            @if doc.is_some() || extras.len() > 0 {
                details[open=open] {
                    summary {
                        @InlineCodeWithSource { code, source_href }
                    }
                    div[class="item-doc"] { @doc }
                    div[style = "padding-left:1.5rem;"] {
                        @for extra in extras {
                            @extra
                        }
                    }
                }
            } else {
                @InlineCodeWithSource { code, source_href }
            }
        }
    }

    StructUnionEnumContent<
        'title,
        Field: markup::Render,
        FieldDoc: markup::Render,
        Traits: markup::Render
    > (title: &'title str, fields: Vec<(Field, FieldDoc)>, traits: Traits) {
        @if fields.len() > 0 {
            section {
                h2[class="border-bottom pb-1"] { @title }
                @for (field, doc) in fields {
                    code { @field }
                    p { @doc }
                }
            }
        }
        section {
            @traits
        }
    }

    ModulePageContent<
        ImportItem: markup::Render,
        ImportItemDoc: markup::Render,
        ModuleItem: markup::Render,
        ModuleItemDoc: markup::Render,
        UnionItem: markup::Render,
        UnionItemDoc: markup::Render,
        StructItem: markup::Render,
        StructItemDoc: markup::Render,
        EnumItem: markup::Render,
        EnumItemDoc: markup::Render,
        FunctionItem: markup::Render,
        FunctionItemDoc: markup::Render,
        TraitItem: markup::Render,
        TraitItemDoc: markup::Render,
        TraitAliasItem: markup::Render,
        TraitAliasItemDoc: markup::Render,
        TypedefItem: markup::Render,
        TypedefItemDoc: markup::Render,
        ConstantItem: markup::Render,
        ConstantItemDoc: markup::Render,
        MacroItem: markup::Render,
        MacroItemDoc: markup::Render,
        ProcMacroItem: markup::Render,
        ProcMacroItemDoc: markup::Render,
    > (
        imports: Vec<(ImportItem, ImportItemDoc)>,
        modules: Vec<(ModuleItem, ModuleItemDoc)>,
        unions: Vec<(UnionItem, UnionItemDoc)>,
        structs: Vec<(StructItem, StructItemDoc)>,
        enums: Vec<(EnumItem, EnumItemDoc)>,
        functions: Vec<(FunctionItem, FunctionItemDoc)>,
        traits: Vec<(TraitItem, TraitItemDoc)>,
        trait_alias: Vec<(TraitAliasItem, TraitAliasItemDoc)>,
        typedefs: Vec<(TypedefItem, TypedefItemDoc)>,
        constants: Vec<(ConstantItem, ConstantItemDoc)>,
        macros: Vec<(MacroItem, MacroItemDoc)>,
        proc_macros: Vec<(ProcMacroItem, ProcMacroItemDoc)>,
    ) {
        @ModuleSection { name: IMPORTS, id: IMPORTS_ID, items: imports }
        @ModuleSection { name: MACROS, id: MACROS_ID, items: macros }
        @ModuleSection { name: PROC_MACROS, id: PROC_MACROS_ID, items: proc_macros }
        @ModuleSection { name: MODULES, id: MODULES_ID, items: modules }
        @ModuleSection { name: UNIONS, id: UNIONS_ID, items: unions }
        @ModuleSection { name: STRUCTS, id: STRUCTS_ID, items: structs }
        @ModuleSection { name: ENUMS, id: ENUMS_ID, items: enums }
        @ModuleSection { name: FUNCTIONS, id: FUNCTIONS_ID, items: functions }
        @ModuleSection { name: TRAITS, id: TRAITS_ID, items: traits }
        @ModuleSection { name: TRAIT_ALIAS, id: TRAIT_ALIAS_ID, items: trait_alias }
        @ModuleSection { name: TYPEDEFS, id: TYPEDEFS_ID, items: typedefs }
        @ModuleSection { name: CONSTANTS, id: CONSTANTS_ID, items: constants }
    }

    TraitPageContent<Summary: markup::Render, Body: markup::Render, Trait: markup::Render>(
        associated_types: Vec<CodeEnchanted<Summary, Body>>,
        associated_consts: Vec<CodeEnchanted<Summary, Body>>,
        required_methods: Vec<CodeEnchanted<Summary, Body>>,
        provided_methods: Vec<CodeEnchanted<Summary, Body>>,
        implementations_foreign_types: Vec<Trait>,
        implementors: Vec<Trait>,
        auto_implementors: Vec<Trait>,
    ) {
        @GeneralSection { name: ASSOCIATED_TYPES, id: ASSOCIATED_TYPES_ID, items: associated_types }
        @GeneralSection { name: ASSOCIATED_CONSTS, id: ASSOCIATED_CONSTS_ID, items: associated_consts }
        @GeneralSection { name: REQUIRED_METHODS, id: REQUIRED_METHODS_ID, items: required_methods }
        @GeneralSection { name: PROVIDED_METHODS, id: PROVIDED_METHODS_ID, items: provided_methods }
        @GeneralSection { name: IMPLEMENTATION_FOREIGN_TYPES, id: IMPLEMENTATION_FOREIGN_TYPES_ID, items: implementations_foreign_types }
        @GeneralSection { name: IMPLEMENTORS, id: IMPLEMENTORS_ID, items: implementors }
        @GeneralSection { name: AUTO_IMPLEMENTORS, id: AUTO_IMPLEMENTORS_ID, items: auto_implementors }
    }

    TraitsWithItems<Trait: markup::Render>(
        implementations: Vec<Trait>,
        trait_implementations: Vec<Trait>,
        auto_trait_implementations: Vec<Trait>,
        blanket_implementations: Vec<Trait>,
    ) {
        @GeneralSection { name: IMPLEMENTATIONS, id: IMPLEMENTATIONS_ID, items: implementations }
        @GeneralSection { name: TRAIT_IMPLEMENTATIONS, id: TRAIT_IMPLEMENTATIONS_ID, items: trait_implementations }
        @GeneralSection { name: AUTO_TRAIT_IMPLEMENTATIONS, id: AUTO_TRAIT_IMPLEMENTATIONS_ID, items: auto_trait_implementations }
        @GeneralSection { name: BLANKET_IMPLEMENTATIONS, id: BLANKET_IMPLEMENTATIONS_ID, items: blanket_implementations }
    }

    ItemLink<'a, Item: markup::Render>(name: Item, link: &'a str, class: &'a str) {
        a[href = link, class = class] {
            @name
        }
    }

    Header<'a>(krate_name: &'a str, rust: &'a str, krate_path: &'a str) {
        header[class="navbar navbar-expand-md navbar-dark rd-navbar"] {
          nav[class="container-xxl flex-wrap flex-md-nowrap", "aria-label"="Main navigation"] {
            a[class="navbar-brand p-0 me-2", href=krate_path, "aria-label"="Rust"] {
              img[src=rust, width="40", height="40", alt="Rust Logo"];
            }

            button[class="navbar-toggler", type="button", "data-bs-toggle"="collapse", "data-bs-target"="#rdNavbar",
                "aria-controls"="rdNavbar", "aria-expanded"="false", "aria-label"="Toggle navigation"] {
              i[class="bi bi-list"] {}
            }

            div[class="collapse navbar-collapse", id="rdNavbar"] {
              ul[class="navbar-nav flex-row flex-wrap pt-2 py-md-0"] {
                li[class="nav-item col-6 col-md-auto"] {
                  a[class="nav-link p-2 active", href=krate_path] { @krate_name }
                }
                /*li[class="nav-item col-6 col-md-auto"] {
                  a[class="nav-link p-2", href="#", title="Not Yet Working"] { "Examples" }
                }
                li[class="nav-item col-6 col-md-auto"] {
                  a[class="nav-link p-2", href="#", title="Not Yet Working"] { "?????" }
                }*/
              }

              hr[class="d-md-none text-white-50"];

              ul[class="navbar-nav flex-row flex-wrap ms-md-auto"] {
                li[class="nav-item col-6 col-md-auto"] {
                  a[class="nav-link p-2", href="#themes", title="Unimplmented"] {
                    i[class="bi bi-palette"] {}
                    small[class="d-md-none ms-2"] { "Themes" }
                  }
                }
                li[class="nav-item col-6 col-md-auto", title="Unimplemented"] {
                  a[class="nav-link p-2", href="#shortcuts"] {
                    i[class="bi bi-question-lg"] {}
                    small[class="d-md-none ms-2"] { "Shortcut" }
                  }
                }
                li[class="nav-item col-6 col-md-auto", title="Unimplemented"] {
                  a[class="nav-link p-2", href="#options"] {
                    i[class="bi bi-wrench"] {}
                    small[class="d-md-none ms-2"] { "Options" }
                  }
                }
              }
            }
          }
        }
    }

    Search<'a>(krate_name: &'a str) {
        nav[class="rd-subnavbar py-2", "aria-label"="Secondary navigation"] {
            div[class="container-xxl d-flex align-items-md-center"] {
                form[class="rd-search position-relative", id="rd-search-form"] {
                    span[class="w-100", style="position: relative; display: inline-block; direction: ltr;"] {
                        input[type="search", class="form-control ds-input", id="rd-search-input", placeholder=format!("Search in {}...", krate_name), "aria-label"="Search docs for...", autocomplete="off", spellcheck="false", role="combobox", "aria-autocomplete"="list", "aria-expanded"="false", "aria-owns"="rd-search-menu", style="position: relative; vertical-align: top;", dir="auto"];
                        span[class="ds-dropdown-menu", style="position: absolute; top: 100%; z-index: 100; display: none; left: 0px; right: auto;", role="listbox", id="rd-search-menu"] {
                            div[class="rd-search-items", id="rd-search-items"] {}
                        }
                    }
                }
                button[class="btn rd-sidebar-toggle d-md-none py-0 px-1 ms-3 order-3 collapsed", type="button", "data-bs-toggle"="collapse", "data-bs-target"="#rd-docs-nav", "aria-controls"="rd-docs-nav", "aria-expanded"="false", "aria-label"="Toggle docs navigation"] {
                    i[class="bi bi-arrows-expand"] {}
                    i[class="bi bi-arrows-collapse"] {}
                }
            }
        }
    }

    Footer(year: u32) {
        footer[class = "container-xxl text-center"] {
            "The rd developpers - (c) " @year
        }
    }
}

fn base_page<'context>(
    global_context: &'context GlobalContext<'context>,
    parent_item_path: Option<&'context ItemPath>,
    item: &'context Item,
) -> Result<(PageContext<'context>, impl Write, &'context str)> {
    let name = item
        .name
        .as_ref()
        .context("unable to get the name of an item")?;

    let krate_path = global_context
        .krate
        .paths
        .get(&item.id)
        .with_context(|| format!("unable to find the path of for the {:?}", &item.id))?;
    let parts = &krate_path.path[..(krate_path.path.len() - 1)];

    let (item_kind_name, _item_kind_file) = item_kind(item);
    let filename: PathBuf = if matches!(krate_path.kind, ItemKind::Module) {
        format!("{}/index.html", name).into()
    } else {
        format!("{}.{}.html", item_kind_name, name).into()
    };

    match &item.inner {
        ItemEnum::Module(_) => {
            let mut path = global_context.output_dir.to_path_buf();
            path.extend(parts);
            path.push(name);

            debug!("creating the module directory {:?}", &path);
            DirBuilder::new()
                .recursive(false)
                .create(&path)
                .context("unable to create the module dir")?;
        }
        _ => {}
    }

    let mut filepath: PathBuf = "".into();
    filepath.extend(parts);
    filepath.push(&filename);

    let filepath = global_context.files.alloc(filepath);

    info!("generating {} {}", item_kind_name, name);
    debug!("creating the {} file {:?}", item_kind_name, filepath);
    trace!(?krate_path, "ID: {:?}", &item.id);

    let path = global_context.output_dir.join(&filepath);
    let file =
        File::create(&path).with_context(|| format!("unable to create the {:?} file", path))?;
    let file = BufWriter::new(file);

    Ok((
        PageContext {
            item,
            filepath,
            filename,
            item_path: global_context.item_paths.alloc({
                let mut path = vec![];
                if let Some(pip) = parent_item_path {
                    path.extend_from_slice(pip.0.as_slice());
                }
                path.push(ItemPathComponent {
                    name: name.clone(),
                    kind: item_kind_name,
                    filepath: filepath.clone(),
                });

                ItemPath(path)
            }),
        },
        file,
        name,
    ))
}

fn item_definition<'context, 'krate>(
    global_context: &'context GlobalContext<'krate>,
    page_context: &'context PageContext<'context>,
    item: &'krate Item,
) -> Result<TokensToHtml<'context, 'krate>> {
    let tokens = pp::Tokens::from_item(item, &global_context.krate.index)?;
    Ok(TokensToHtml(global_context, page_context, tokens))
}

pub(crate) fn module_page<'context>(
    global_context: &'context GlobalContext<'context>,
    parent_item_path: Option<&'context ItemPath>,
    item: &'context Item,
    module: &'context Module,
) -> Result<PageContext<'context>> {
    let (page_context, mut file, module_name) = base_page(global_context, parent_item_path, item)?;

    let mut module_page_content = ModulePageContent {
        imports: Default::default(),
        modules: Default::default(),
        unions: Default::default(),
        structs: Default::default(),
        enums: Default::default(),
        functions: Default::default(),
        traits: Default::default(),
        trait_alias: Default::default(),
        typedefs: Default::default(),
        constants: Default::default(),
        macros: Default::default(),
        proc_macros: Default::default(),
    };

    let filenames = Arena::<PathBuf>::new();

    let mut toc_macros = TocSection {
        name: MACROS,
        id: MACROS_ID,
        items: Default::default(),
    };
    let mut toc_proc_macros = TocSection {
        name: PROC_MACROS,
        id: PROC_MACROS_ID,
        items: Default::default(),
    };
    let mut toc_modules = TocSection {
        name: MODULES,
        id: MODULES_ID,
        items: Default::default(),
    };
    let mut toc_unions = TocSection {
        name: UNIONS,
        id: UNIONS_ID,
        items: Default::default(),
    };
    let mut toc_structs = TocSection {
        name: STRUCTS,
        id: STRUCTS_ID,
        items: Default::default(),
    };
    let mut toc_enums = TocSection {
        name: ENUMS,
        id: ENUMS_ID,
        items: Default::default(),
    };
    let mut toc_traits = TocSection {
        name: TRAITS,
        id: TRAITS_ID,
        items: Default::default(),
    };
    let mut toc_functions = TocSection {
        name: FUNCTIONS,
        id: FUNCTIONS_ID,
        items: Default::default(),
    };
    let mut toc_typedefs = TocSection {
        name: TYPEDEFS,
        id: TYPEDEFS_ID,
        items: Default::default(),
    };
    let mut toc_constants = TocSection {
        name: CONSTANTS,
        id: CONSTANTS_ID,
        items: Default::default(),
    };

    for id in &module.items {
        let item = global_context
            .krate
            .index
            .get(id)
            .with_context(|| format!("Unable to find the item {:?}", id))?;

        if !id.0.starts_with("0:") {
            error!("ignoring for now `pub use item`: {:?}", id);
            continue;
        }

        let summary_line_doc = markdown::MarkdownSummaryLine::from_docs(
            &global_context.krate,
            &item.docs,
            &item.links,
        );

        match &item.inner {
            ItemEnum::Import(_) => {
                module_page_content.imports.push((
                    TokensToHtml(
                        global_context,
                        &page_context,
                        pp::Tokens::from_item(item, &global_context.krate.index)?,
                    ),
                    Option::<String>::None,
                ));
            }
            ItemEnum::Union(union_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of union")?;

                let page_context =
                    union_page(global_context, page_context.item_path, &item, union_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_unions
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.unions.push((
                    ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "union",
                    },
                    summary_line_doc,
                ));
            }
            ItemEnum::Struct(struct_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of struct")?;

                let page_context =
                    struct_page(global_context, page_context.item_path, &item, struct_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_structs
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.structs.push((
                    ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "struct",
                    },
                    summary_line_doc,
                ));
            }
            ItemEnum::Enum(enum_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of enum")?;

                let page_context = enum_page(global_context, page_context.item_path, &item, enum_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_enums
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.enums.push((
                    ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "enum",
                    },
                    summary_line_doc,
                ));
            }
            ItemEnum::Function(function_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of function")?;

                let page_context =
                    function_page(global_context, &page_context.item_path, &item, function_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_functions
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.functions.push((
                    ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "fn",
                    },
                    summary_line_doc,
                ));
            }
            ItemEnum::Trait(trait_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of trait")?;

                let page_context =
                    trait_page(global_context, &page_context.item_path, &item, trait_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_traits
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.traits.push((
                    ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "trait",
                    },
                    summary_line_doc,
                ));
            }
            ItemEnum::TraitAlias(_) => {
                module_page_content.trait_alias.push((
                    TokensToHtml(
                        global_context,
                        &page_context,
                        pp::Tokens::from_item(item, &global_context.krate.index)?,
                    ),
                    Option::<String>::None,
                ));
            }
            ItemEnum::Typedef(typedef_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of typedef")?;

                let page_context2 =
                    typedef_page(global_context, &page_context.item_path, &item, typedef_)?;
                let filename = filenames.alloc(page_context2.filename);

                toc_typedefs
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.typedefs.push((
                    ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "typedef",
                    },
                    TokensToHtml(
                        global_context,
                        &page_context,
                        pp::Tokens::from_type(&typedef_.type_)?,
                    ),
                ));
            }
            ItemEnum::Constant(constant_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of constant")?;

                let page_context =
                    constant_page(global_context, page_context.item_path, &item, constant_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_constants
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.constants.push((
                    ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "constant",
                    },
                    summary_line_doc,
                ));
            }
            ItemEnum::Macro(macro_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of macro")?;

                let page_context =
                    macro_page(global_context, page_context.item_path, &item, macro_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_macros
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.macros.push((
                    ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "macro",
                    },
                    summary_line_doc,
                ));
            }
            ItemEnum::ProcMacro(proc_macro_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of proc_macro")?;

                let page_context =
                    proc_macro_page(global_context, page_context.item_path, &item, proc_macro_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_proc_macros
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.proc_macros.push((
                    ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "proc_macro",
                    },
                    summary_line_doc,
                ));
            }
            ItemEnum::Module(module_) => {
                let name = item
                    .name
                    .as_ref()
                    .context("unable to get the name of module")?;
                let page_context =
                    module_page(global_context, Some(page_context.item_path), &item, module_)?;
                let filename = filenames.alloc(page_context.filename);

                toc_modules
                    .items
                    .push((Cow::Borrowed(name), TocDestination::File(filename)));
                module_page_content.modules.push((
                    ItemLink {
                        name,
                        link: filename.to_str().with_context(|| {
                            format!("unable to convert PathBuf {:?} to str", filename)
                        })?,
                        class: "mod",
                    },
                    summary_line_doc,
                ));
            }
            _ => todo!("don't know what to do"),
        }
    }

    let is_top_level = parent_item_path.is_none();
    let mut doc_toc = Default::default();

    let page = Base {
        infos: BodyInformations::with(global_context, &page_context),
        main: ItemPage {
            item_type: if is_top_level { "Crate" } else { "Module" },
            item_name: module_name,
            item_path: page_context.item_path.display(&page_context),
            item_definition: Option::<String>::None,
            item_doc: markdown::MarkdownWithToc::from_docs(
                global_context,
                &page_context,
                &item.docs,
                &item.links,
                &mut doc_toc,
            ),
            toc: &vec![
                toc_modules,
                toc_macros,
                toc_unions,
                toc_structs,
                toc_enums,
                toc_functions,
                toc_traits,
                toc_typedefs,
                toc_constants,
                toc_proc_macros,
            ],
            content: Some(module_page_content),
        },
    };

    writeln!(file, "{}", page)?;

    Ok(page_context)
}

fn trait_page<'context>(
    global_context: &'context GlobalContext<'context>,
    parent_item_path: &'context ItemPath,
    item: &'context Item,
    trait_: &'context Trait,
) -> Result<PageContext<'context>> {
    let (page_context, mut file, name) = base_page(global_context, Some(parent_item_path), item)?;
    let definition = item_definition(global_context, &page_context, item)?;

    let mut trait_page_content = TraitPageContent {
        associated_types: Default::default(),
        associated_consts: Default::default(),
        required_methods: Default::default(),
        provided_methods: Default::default(),
        implementations_foreign_types: Default::default(),
        implementors: Default::default(),
        auto_implementors: Default::default(),
    };

    let mut toc_associated_types = TocSection {
        name: ASSOCIATED_TYPES,
        id: ASSOCIATED_TYPES_ID,
        items: vec![],
    };
    let mut toc_associated_consts = TocSection {
        name: ASSOCIATED_CONSTS,
        id: ASSOCIATED_CONSTS_ID,
        items: vec![],
    };
    let mut toc_required_methods = TocSection {
        name: REQUIRED_METHODS,
        id: REQUIRED_METHODS_ID,
        items: vec![],
    };
    let mut toc_provided_methods = TocSection {
        name: PROVIDED_METHODS,
        id: PROVIDED_METHODS_ID,
        items: vec![],
    };
    let mut toc_implementation_foreign_types = TocSection {
        name: IMPLEMENTATION_FOREIGN_TYPES,
        id: IMPLEMENTATION_FOREIGN_TYPES_ID,
        items: vec![],
    };
    let mut toc_implementors = TocSection {
        name: IMPLEMENTORS,
        id: IMPLEMENTORS_ID,
        items: vec![],
    };

    for id in &trait_.items {
        let item = global_context
            .krate
            .index
            .get(id)
            .with_context(|| format!("Unable to find the item {:?}", id))?;

        match &item.inner {
            ItemEnum::Method(method_) => {
                let (toc, who) = if method_.has_body {
                    (
                        &mut toc_provided_methods,
                        &mut trait_page_content.provided_methods,
                    )
                } else {
                    (
                        &mut toc_required_methods,
                        &mut trait_page_content.required_methods,
                    )
                };

                who.push(CodeEnchanted::from_item(
                    global_context,
                    &page_context,
                    toc,
                    &item,
                    false,
                )?);
            }
            ItemEnum::AssocConst { .. } => {
                trait_page_content
                    .associated_consts
                    .push(CodeEnchanted::from_item(
                        global_context,
                        &page_context,
                        &mut toc_associated_consts,
                        &item,
                        false,
                    )?);
            }
            ItemEnum::AssocType { .. } => {
                trait_page_content
                    .associated_types
                    .push(CodeEnchanted::from_item(
                        global_context,
                        &page_context,
                        &mut toc_associated_types,
                        &item,
                        false,
                    )?);
            }
            _ => warn!("ignore {:?}", item.inner),
        }
    }

    let impls = trait_
        .implementors
        .iter()
        .map(|id| {
            let item = global_context
                .krate
                .index
                .get(id)
                .with_context(|| format!("Unable to find the item {:?}", id))?;

            Ok((
                item,
                match &item.inner {
                    ItemEnum::Impl(impl_) => impl_,
                    _ => Err(anyhow::anyhow!(
                        "impl id is not impl in struct_union_content"
                    ))?,
                },
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    for (item, impl_) in &impls {
        let (toc, who) = match type_id(&impl_.for_) {
            Ok(id) if !id.0.starts_with("0:") => (
                &mut toc_implementation_foreign_types,
                &mut trait_page_content.implementations_foreign_types,
            ),
            Err(Some(ItemKind::Primitive)) => (
                &mut toc_implementation_foreign_types,
                &mut trait_page_content.implementations_foreign_types,
            ),
            _ => (&mut toc_implementors, &mut trait_page_content.implementors),
        };

        who.push(CodeEnchantedWithExtras::from_items(
            global_context,
            &page_context,
            Some(toc),
            None,
            item,
            impl_,
            false,
        )?);
    }

    let mut doc_toc = Default::default();
    let page = Base {
        infos: BodyInformations::with(global_context, &page_context),
        main: ItemPage {
            item_type: "Trait",
            item_name: name,
            item_definition: Some(definition),
            //item_path: &page_context.item_path,
            item_path: page_context.item_path.display(&page_context),
            item_doc: markdown::MarkdownWithToc::from_docs(
                global_context,
                &page_context,
                &item.docs,
                &item.links,
                &mut doc_toc,
            ),
            toc: &vec![
                toc_associated_types,
                toc_associated_consts,
                toc_required_methods,
                toc_provided_methods,
                toc_implementation_foreign_types,
                toc_implementors,
                //toc_auto_implementors,
            ],
            content: Some(trait_page_content),
        },
    };

    writeln!(file, "{}", page)?;

    Ok(page_context)
}

macro_rules! ç {
    ($ty:ty => $fn:ident $type:literal $title:literal $fields:ident) => {
        fn $fn<'context>(
            global_context: &'context GlobalContext<'context>,
            parent_item_path: &'context ItemPath,
            item: &'context Item,
            inner: &'context $ty,
        ) -> Result<PageContext<'context>> {
            let (page_context, mut file, name) =
                base_page(global_context, Some(parent_item_path), item)?;
            let definition = item_definition(global_context, &page_context, item)?;

            let (toc, content) = struct_union_enum_content(
                global_context,
                &page_context,
                $title,
                &inner.$fields,
                &inner.impls,
            )?;

            let mut doc_toc = Default::default();
            let page = Base {
                infos: BodyInformations::with(global_context, &page_context),
                main: ItemPage {
                    item_type: $type,
                    item_name: name,
                    item_definition: Some(definition),
                    item_path: page_context.item_path.display(&page_context),
                    item_doc: markdown::MarkdownWithToc::from_docs(
                        global_context,
                        &page_context,
                        &item.docs,
                        &item.links,
                        &mut doc_toc,
                    ),
                    toc: &toc,
                    content: Some(content),
                },
            };

            writeln!(file, "{}", page)?;

            Ok(page_context)
        }
    };
}

macro_rules! é {
    ($ty:ty => $fn:ident $type:literal) => {
        fn $fn<'context>(
            global_context: &'context GlobalContext<'context>,
            parent_item_path: &'context ItemPath,
            item: &'context Item,
            #[allow(unused)] inner: &'context $ty,
        ) -> Result<PageContext<'context>> {
            let (page_context, mut file, name) = base_page(global_context, Some(parent_item_path), item)?;
            let definition = item_definition(global_context, &page_context, item)?;

            let mut doc_toc = Default::default();
            let page = Base {
                infos: BodyInformations::with(global_context, &page_context),
                main: ItemPage {
                    item_type: $type,
                    item_name: name,
                    item_definition: Some(definition),
                    item_path: page_context.item_path.display(&page_context),
                    item_doc: markdown::MarkdownWithToc::from_docs(
                        global_context,
                        &page_context,
                        &item.docs,
                        &item.links,
                        &mut doc_toc,
                    ),
                    toc: /* TODO: Optional */ &vec![],
                    content: Option::<String>::None,
                },
            };

            writeln!(file, "{}", page)?;

            Ok(page_context)
        }
    };
}

type Macro = String;

ç!(Union => union_page "Union" "Fields" fields);
ç!(Struct => struct_page "Struct" "Fields" fields);
ç!(Enum => enum_page "Enum" "Variants" variants);
é!(Typedef => typedef_page "Type Definition");
é!(Macro => macro_page "Macro");
é!(ProcMacro => proc_macro_page "Proc-Macro");
é!(Function => function_page "Function");
é!(Constant => constant_page "Constant");

impl<'context, 'krate>
    CodeEnchanted<TokensToHtml<'context, 'krate>, markdown::Markdown<'context, 'krate, 'context>>
{
    fn from_item(
        global_context: &'context GlobalContext<'krate>,
        page_context: &'context PageContext<'context>,
        toc_section: &mut TocSection<'context>,
        item: &'krate Item,
        open: bool,
    ) -> Result<Self> {
        let id = if let Some((name, id)) = id(&global_context.krate, item) {
            toc_section
                .items
                .push((name, TocDestination::Id(id.clone())));
            Some(id)
        } else {
            None
        };

        Ok(Self {
            code: TokensToHtml(
                global_context,
                page_context,
                pp::Tokens::from_item(&item, &global_context.krate.index)?,
            ),
            doc: markdown::Markdown::from_docs(
                &global_context,
                &page_context,
                &item.docs,
                &item.links,
            ),
            id,
            open,
            source_href: Option::<String>::None,
        })
    }

    fn from_item_without_id(
        global_context: &'context GlobalContext<'krate>,
        page_context: &'context PageContext<'context>,
        item: &'krate Item,
    ) -> Result<Self> {
        Ok(Self {
            code: TokensToHtml(
                global_context,
                page_context,
                pp::Tokens::from_item(&item, &global_context.krate.index)?,
            ),
            doc: markdown::Markdown::from_docs(
                &global_context,
                &page_context,
                &item.docs,
                &item.links,
            ),
            open: false,
            id: Option::<String>::None,
            source_href: Option::<String>::None,
        })
    }
}

impl<'context, 'krate>
    CodeEnchantedWithExtras<
        TokensToHtml<'context, 'krate /*, 'tokens*/>,
        markdown::Markdown<'context, 'krate, 'context>,
        CodeEnchanted<
            TokensToHtml<'context, 'krate /*, 'tokens*/>,
            markdown::Markdown<'context, 'krate, 'context>,
        >,
    >
{
    fn from_items(
        global_context: &'context GlobalContext<'krate>,
        page_context: &'context PageContext<'context>,
        toc_top_section: Option<&mut TocSection<'context>>,
        toc_sub_section: Option<&mut TocSection<'context>>,
        item: &'krate Item,
        impl_: &'krate Impl,
        open: bool,
    ) -> Result<Self> {
        let id = if let Some(toc_top_section) = toc_top_section {
            if let Some((name, id)) = id(&global_context.krate, item) {
                toc_top_section
                    .items
                    .push((name, TocDestination::Id(id.clone())));
                Some(id)
            } else {
                None
            }
        } else {
            None
        };

        let mut toc_sub_section = toc_sub_section;

        Ok(CodeEnchantedWithExtras {
            code: TokensToHtml(
                global_context,
                page_context,
                pp::Tokens::from_item(item, &global_context.krate.index)?,
            ),
            doc: markdown::Markdown::from_docs(
                &global_context,
                &page_context,
                &item.docs,
                &item.links,
            ),
            id,
            open,
            source_href: Option::<String>::None,
            extras: impl_
                .items
                .iter()
                .map(|id| {
                    let item = global_context
                        .krate
                        .index
                        .get(id)
                        .with_context(|| format!("Unable to find the item {:?}", id))?;

                    if let Some(toc_sub_section) = &mut toc_sub_section {
                        CodeEnchanted::from_item(
                            global_context,
                            page_context,
                            toc_sub_section,
                            &item,
                            open,
                        )
                    } else {
                        CodeEnchanted::from_item_without_id(global_context, page_context, &item)
                    }
                })
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

fn is_auto_trait<'krate>(krate: &'krate Crate, id: &'krate Id) -> Result<(bool, u32)> {
    let item = krate
        .index
        .get(id)
        .with_context(|| format!("Unable to find the item {:?}", id))?;

    Ok(match &item.inner {
        ItemEnum::Trait(trait_) => (trait_.is_auto, item.crate_id),
        _ => Err(anyhow::anyhow!("is_auto_trait: error not an trait"))?,
    })
}

fn struct_union_enum_content<'context, 'krate, 'title>(
    global_context: &'context GlobalContext<'krate>,
    page_context: &'context PageContext<'context>,
    title: &'title str,
    fields: &Vec<Id>,
    impls: &Vec<Id>,
) -> Result<(
    Vec<TocSection<'context>>,
    StructUnionEnumContent<
        'title,
        TokensToHtml<'context, 'krate /*, 'tokens*/>,
        Option<markdown::Markdown<'context, 'krate, 'context>>,
        TraitsWithItems<
            CodeEnchantedWithExtras<
                TokensToHtml<'context, 'krate /*, 'tokens*/>,
                markdown::Markdown<'context, 'krate, 'context>,
                CodeEnchanted<
                    TokensToHtml<'context, 'krate /*, 'tokens*/>,
                    markdown::Markdown<'context, 'krate, 'context>,
                >,
            >,
        >,
    >,
)> {
    let impls = impls
        .iter()
        .map(|id| {
            let item = global_context
                .krate
                .index
                .get(id)
                .with_context(|| format!("Unable to find the item {:?}", id))?;

            Ok((
                item,
                match &item.inner {
                    ItemEnum::Impl(impl_) => impl_,
                    _ => Err(anyhow::anyhow!(
                        "impl id is not impl in struct_union_content"
                    ))?,
                },
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    let mut toc_methods = TocSection {
        name: "Methods",
        id: "methods",
        items: vec![],
    };
    let mut toc_traits = TocSection {
        name: TRAIT_IMPLEMENTATIONS,
        id: TRAIT_IMPLEMENTATIONS_ID,
        items: vec![],
    };
    let mut toc_auto_traits = TocSection {
        name: AUTO_TRAIT_IMPLEMENTATIONS,
        id: AUTO_TRAIT_IMPLEMENTATIONS_ID,
        items: vec![],
    };
    let mut toc_blanket_traits = TocSection {
        name: BLANKET_IMPLEMENTATIONS,
        id: BLANKET_IMPLEMENTATIONS_ID,
        items: vec![],
    };

    // TODO: Move all the filtering logic directly in the map above
    let content = StructUnionEnumContent {
        title,
        fields: fields
            .iter()
            .map(|id| {
                let item = global_context
                    .krate
                    .index
                    .get(id)
                    .with_context(|| format!("Unable to find the item {:?}", id))?;

                Ok((
                    TokensToHtml(
                        global_context,
                        page_context,
                        pp::Tokens::from_item(&item, &global_context.krate.index)?,
                    ),
                    markdown::Markdown::from_docs(
                        global_context,
                        page_context,
                        &item.docs,
                        &item.links,
                    ),
                ))
            })
            .collect::<Result<Vec<_>>>()?,
        traits: TraitsWithItems {
            implementations: impls
                .iter()
                .filter(|(_item, impl_)| matches!(impl_.trait_, None))
                .map(|(item, impl_)| {
                    CodeEnchantedWithExtras::from_items(
                        global_context,
                        page_context,
                        None,
                        Some(&mut toc_methods),
                        item,
                        impl_,
                        true,
                    )
                })
                .collect::<Result<Vec<_>>>()?,
            trait_implementations: impls
                .iter()
                .filter_map(|(item, impl_)| match (&impl_.trait_, &impl_.blanket_impl) {
                    (Some(type_), None) => match type_ {
                        Type::ResolvedPath { id, .. } => {
                            match is_auto_trait(&global_context.krate, &id) {
                                Ok((false, _)) => Some(CodeEnchantedWithExtras::from_items(
                                    global_context,
                                    page_context,
                                    Some(&mut toc_traits),
                                    None,
                                    item,
                                    impl_,
                                    false,
                                )),
                                Err(e) => Some(Err(e)),
                                _ => None,
                            }
                        }
                        _ => None,
                    },
                    _ => None,
                })
                .collect::<Result<Vec<_>>>()?,
            auto_trait_implementations: impls
                .iter()
                .filter_map(|(item, impl_)| match (&impl_.trait_, &impl_.blanket_impl) {
                    (Some(type_), None) => match type_ {
                        Type::ResolvedPath { id, .. } => {
                            match is_auto_trait(&global_context.krate, &id) {
                                Ok((true, _)) => Some(CodeEnchantedWithExtras::from_items(
                                    global_context,
                                    page_context,
                                    Some(&mut toc_auto_traits),
                                    None,
                                    item,
                                    impl_,
                                    false,
                                )),
                                Err(e) => Some(Err(e)),
                                _ => None,
                            }
                        }
                        _ => None,
                    },
                    _ => None,
                })
                .collect::<Result<Vec<_>>>()?,
            blanket_implementations: impls
                .iter()
                .filter(|(_item, impl_)| matches!(impl_.blanket_impl, Some(_)))
                .map(|(item, impl_)| {
                    CodeEnchantedWithExtras::from_items(
                        global_context,
                        page_context,
                        Some(&mut toc_blanket_traits),
                        None,
                        item,
                        impl_,
                        false,
                    )
                })
                .collect::<Result<Vec<_>>>()?,
        },
    };

    Ok((
        vec![toc_methods, toc_traits, toc_auto_traits, toc_blanket_traits],
        content,
    ))
}

fn item_kind2(kind: &ItemKind) -> (&'static str, bool) {
    match kind {
        ItemKind::Module => ("mod", true),
        ItemKind::Import => ("import", true),
        ItemKind::Union => ("union", true),
        ItemKind::Struct => ("struct", true),
        ItemKind::StructField => ("struct.field", false),
        ItemKind::Enum => ("enum", true),
        ItemKind::Variant => ("enum.variant", false),
        ItemKind::Function => ("fn", true),
        ItemKind::Trait => ("trait", true),
        ItemKind::TraitAlias => ("trait.alias", true),
        ItemKind::Method => ("method", false),
        ItemKind::Impl => ("impl", false),
        ItemKind::Typedef => ("typedef", true),
        ItemKind::Constant => ("const", true),
        ItemKind::Static => ("static", true),
        ItemKind::Macro => ("macro", true),
        //ItemKind::ProcMacro => ("proc.macro", true),
        ItemKind::AssocConst => ("assoc.const", false),
        ItemKind::AssocType => ("assoc.type", false),
        _ => unimplemented!(),
    }
}

fn item_kind(item: &Item) -> (&'static str, bool) {
    match &item.inner {
        ItemEnum::Module(_) => ("mod", true),
        ItemEnum::Import(_) => ("import", true),
        ItemEnum::Union(_) => ("union", true),
        ItemEnum::Struct(_) => ("struct", true),
        ItemEnum::StructField(_) => ("struct.field", false),
        ItemEnum::Enum(_) => ("enum", true),
        ItemEnum::Variant(_) => ("enum.variant", false),
        ItemEnum::Function(_) => ("fn", true),
        ItemEnum::Trait(_) => ("trait", true),
        ItemEnum::TraitAlias(_) => ("trait.alias", true),
        ItemEnum::Method(_) => ("method", false),
        ItemEnum::Impl(_) => ("impl", false),
        ItemEnum::Typedef(_) => ("typedef", true),
        ItemEnum::Constant(_) => ("const", true),
        ItemEnum::Static(_) => ("static", true),
        ItemEnum::Macro(_) => ("macro", true),
        ItemEnum::ProcMacro(_) => ("proc.macro", true),
        ItemEnum::AssocConst { .. } => ("assoc.const", false),
        ItemEnum::AssocType { .. } => ("assoc.type", false),
        _ => unimplemented!(),
    }
}

fn id<'krate>(krate: &'krate Crate, item: &'krate Item) -> Option<(Cow<'krate, str>, String)> {
    if let Some(name) = &item.name {
        let (item_kind_name, is_file) = item_kind(item);

        // TODO: This seems to be another bug with the json where inner assoc type are typedef
        // whitch is clearly wrong!
        assert_eq!(
            false,
            is_file && !matches!(&item.inner, ItemEnum::Typedef(_))
        );
        Some((Cow::Borrowed(name), format!("{}.{}", item_kind_name, name)))
    } else if let ItemEnum::Impl(impl_) = &item.inner {
        let mut name = String::new();
        let mut id = String::new();

        for token in pp::Tokens::from_item(item, &krate.index)
            .unwrap()
            .into_iter()
        {
            match token {
                pp::Token::Ponct(_) => id.push('-'),
                pp::Token::Ident(ident, _) => id.push_str(ident),
                pp::Token::Kw(kw) => id.push_str(kw),
                _ => {}
            }
        }

        let name_type = match &impl_.trait_ {
            Some(type_) => match type_ {
                Type::ResolvedPath { id, .. } if !id.0.starts_with("0:") => {
                    if impl_.negative {
                        name.push_str("!");
                    }
                    type_
                }
                _ => &impl_.for_,
            },
            None => &impl_.for_,
        };

        for token in pp::Tokens::from_type(name_type).unwrap().into_iter() {
            match token {
                pp::Token::Ponct(p) => name.push_str(p),
                pp::Token::Ident(ident, _) => name.push_str(ident),
                pp::Token::Kw(kw) => name.push_str(kw),
                pp::Token::Special(s) if *s == pp::SpecialToken::Space => name.push(' '),
                _ => {}
            }
        }

        Some((Cow::Owned(name), id))
    } else {
        None
    }
}

fn type_id(type_: &Type) -> Result<&Id, Option<ItemKind>> {
    match type_ {
        Type::ResolvedPath { id, .. } => Ok(id),
        Type::BorrowedRef { type_, .. } => type_id(type_),
        Type::RawPointer { type_, .. } => type_id(type_),
        Type::Slice(type_) => type_id(type_),
        Type::Array { type_, .. } => type_id(type_),
        Type::Primitive(..) => Err(Some(ItemKind::Primitive)),
        _ => Err(None),
    }
}

pub(crate) fn relative(base: &PathBuf, url: &PathBuf) -> PathBuf {
    let mut relative = PathBuf::new();

    let ends_with_html = |c: &std::path::Component| -> bool {
        match c {
            std::path::Component::Normal(path) => path
                .to_str()
                .and_then(|s| Some(s.ends_with(".html")))
                .unwrap_or(false),
            _ => false,
        }
    };

    let mut base_components = base
        .components()
        .take_while(|c| !ends_with_html(c))
        .peekable();
    let mut url_components = url
        .components()
        .take_while(|c| !ends_with_html(c))
        .peekable();

    // Skip over the common prefix
    //while dbg!(base_components.peek().is_some()) && dbg!(base_components.peek()) == dbg!(url_components.peek()) {
    while base_components.peek().is_some() && base_components.peek() == url_components.peek() {
        base_components.next();
        url_components.next();
    }

    // Add `..` segments for the remainder of the base path
    for base_path_segment in base_components {
        // Skip empty last segments
        if let std::path::Component::Normal(s) = base_path_segment {
            if s.is_empty() {
                break;
            }
        }

        relative.push("..");
    }

    // Append the remainder of the other URI
    for url_path_segment in url_components {
        relative.push(url_path_segment);
    }

    let url_file_name = url.file_name();
    if let Some(url_file_name) = url_file_name {
        relative.push(url_file_name);
    }

    relative
}

fn top_of(base: &PathBuf) -> PathBuf {
    let mut relative = PathBuf::new();

    // Add `..` segments for the remainder of the base path
    for base_path_segment in base.components() {
        // Skip empty last segments
        if let std::path::Component::Normal(s) = base_path_segment {
            if s.is_empty()
                || s.to_str()
                    .and_then(|s| Some(s.ends_with(".html")))
                    .unwrap_or(false)
            {
                break;
            }
        }

        relative.push("..");
    }

    relative
}

pub(crate) fn href<'context, 'krate>(
    global_context: &'context GlobalContext<'krate>,
    page_context: &'context PageContext<'context>,
    id: &'krate Id,
) -> Option<(
    Option<&'context String>,
    PathBuf,
    Option<String>,
    &'static str,
)> {
    let to = global_context.krate.paths.get(id);

    if to.is_none() {
        if let Some(item) = global_context.krate.index.get(id) {
            match &item.inner {
                ItemEnum::Method { .. } => {
                    return Some((
                        None,
                        "".into(),
                        Some(format!("method.{}", item.name.as_ref().unwrap())),
                        "method",
                    ))
                }
                _ => warn!("not handling this kind of items"),
            }
        } else {
            warn!(?id, "id not in paths or index");
        }
        return None;
    }

    let to = to.unwrap();
    let (to_kind, to_always_file) = item_kind2(&to.kind);

    if to_always_file {
        let parts = &to.path[..(to.path.len()
            - if !matches!(to.kind, ItemKind::Module) {
                1
            } else {
                0
            })];

        let filename: PathBuf = if matches!(to.kind, ItemKind::Module) {
            "index.html".into()
        } else {
            format!("{}.{}.html", to_kind, to.path[to.path.len() - 1]).into()
        };

        let mut dest = PathBuf::with_capacity(30);
        dest.extend(parts);
        dest.push(filename);

        //debug!(?dest, ?current_filepath, ?relative);

        let (external_crate_url, path) =
            if let Some(external_crate) = global_context.krate.external_crates.get(&to.crate_id) {
                if let Some(html_root_url) = &external_crate.html_root_url {
                    (Some(html_root_url), dest)
                } else {
                    return None;
                }
            } else {
                let current_filepath = &page_context.filepath;
                (None, relative(current_filepath, &dest))
            };

        Some((external_crate_url, path, None, to_kind))
    } else {
        trace!(?to_kind, "not is_always_file");
        return None;
    }
}

struct TokensToHtml<'context, 'krate>(
    &'context GlobalContext<'krate>,
    &'context PageContext<'context>,
    pp::Tokens<'krate>,
);

impl<'context, 'krate /*, 'tokens */> markup::Render
    for TokensToHtml<'context, 'krate /*, 'tokens*/>
{
    fn render(&self, writer: &mut impl std::fmt::Write) -> std::fmt::Result {
        let mut in_where_clause = false;
        for token in &*self.2 {
            match token {
                pp::Token::Ident(ident, id) => {
                    writer.write_str("<span")?;

                    if let Some(id) = id {
                        writer.write_str(" class=\"ident")?;
                        if let Some((external_crate_url, relative_path, fragment, type_of)) =
                            href(self.0, self.1, id)
                        {
                            writer.write_str(" ")?;
                            writer.write_str(type_of)?;
                            writer.write_str("\">")?;

                            writer.write_str("<a href=\"")?;
                            if let Some(external_crate_url) = external_crate_url {
                                writer.write_str(external_crate_url)?;
                                if !external_crate_url.ends_with("/") {
                                    writer.write_str("/")?;
                                }
                            }
                            writer.write_str(relative_path.to_str().unwrap())?;
                            if let Some(fragment) = fragment {
                                writer.write_str("#")?;
                                writer.write_str(&fragment)?;
                            }
                            writer.write_str("\">")?;
                            writer.write_str(ident)?;
                            writer.write_str("</a>")?;
                        } else {
                            writer.write_str("\">")?;
                            writer.write_str(ident)?;
                        }
                    } else {
                        writer.write_str(">")?;
                        writer.write_str(ident)?;
                    }

                    writer.write_str("</span>")?;
                }
                pp::Token::Kw(kw) => {
                    if *kw == "where" {
                        if in_where_clause {
                            warn!("already in where clause");
                        }
                        in_where_clause = true;
                        writer.write_str("<span class=\"where-clause\">")?;
                    }
                    writer.write_str("<span class=\"kw\">")?;
                    writer.write_str(kw)?;
                    writer.write_str("</span>")?;
                }
                pp::Token::Ponct(ponct) => {
                    writer.write_str("<span class=\"ponct\">")?;
                    match *ponct {
                        ">" => writer.write_str("&gt;")?,
                        "<" => writer.write_str("&lt;")?,
                        "&" => writer.write_str("&amp;")?,
                        _ => writer.write_str(ponct)?,
                    }
                    writer.write_str("</span>")?;
                    if *ponct == ";" && in_where_clause {
                        writer.write_str("</span>")?;
                        in_where_clause = false;
                    }
                }
                pp::Token::Special(special) => match special {
                    pp::SpecialToken::NewLine => writer.write_str("<br>")?,
                    pp::SpecialToken::Space => writer.write_str("&nbsp;")?,
                    pp::SpecialToken::Tabulation => writer.write_str("&nbsp;&nbsp;&nbsp;&nbsp;")?,
                },
                pp::Token::Plain(plain) => writer.write_str(plain)?,
            }
        }
        if in_where_clause {
            writer.write_str("</span>")?;
        }
        Ok(())
    }
}
