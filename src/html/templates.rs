//! HTML templates

use std::ops::Deref;
use std::path::PathBuf;

use super::constants::*;
use super::markdown::MarkdownWithToc;
use super::render::{GlobalContext, PageContext, TocSection};
use super::utils::*;

pub struct BodyInformations<'a> {
    page_title: String,
    krate_name: &'a str,
    root_path: PathBuf,
}

impl<'context, 'krate> BodyInformations<'krate> {
    pub(super) fn with(
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
            root_path: top_of(page_context.filepath),
        }
    }
}

markup::define! {
    Base<'a, Body: markup::Render>(infos: BodyInformations<'a>, main: Body) {
        @markup::doctype()
        html[lang="en", "data-bs-color-scheme"="light"] {
            head {
                title { @infos.page_title }
                meta[charset="utf-8"];
                meta[name="viewport", content="width=device-width, initial-scale=1"];
                meta[name="color-scheme", content="light dark"];
                link[href="https://cdn.jsdelivr.net/npm/bootstrap-dark-5@1.1.3/dist/css/bootstrap-blackbox.min.css", integrity="sha384-nXtYGwAUBOgb4M8Eo9xOK3Er3bVPQo1HguUNWf/RheIagsbCaP3ZaYqVeUqHEr20", rel="stylesheet", crossorigin="anonymous"];
                link[href="https://cdn.jsdelivr.net/npm/bootstrap-icons@1.5.0/font/bootstrap-icons.css", integrity="sha384-tKLJeE1ALTUwtXlaGjJYM3sejfssWdAaWR2s97axw4xkiAdMzQjtOjgcyw0Y50KU", rel="stylesheet", crossorigin="anonymous"];
                link[href=format!("{}/{}", infos.root_path.display(), STYLE_CSS), rel="stylesheet"];
                link[href=format!("{}/{}", infos.root_path.display(), RUST_SVG), rel="icon", type="image/svg+xml"];
            }
            body {
                @Header { krate_name: infos.krate_name, rust: &format!("{}/{}", infos.root_path.display(), RUST_SVG), krate_path: &format!("{}/{}/index.html", infos.root_path.display(), infos.krate_name) }
                @Search { krate_name: infos.krate_name }
                #main[class="container-xxl"] {
                    @main
                }
                @Footer { year: 2022 }
                script[src="https://cdn.jsdelivr.net/npm/bootstrap@5.1.0/dist/js/bootstrap.min.js", integrity="sha384-cn7l7gDp0eyniUwwAZgrzD06kc/tftFf19TOAs2zVinnD/C7E91j9yyk5//jjpt/", crossorigin="anonymous"] {}
                script[src="https://cdn.jsdelivr.net/npm/bootstrap-dark-5@1.1.3/dist/js/darkmode.min.js", integrity="sha384-A4SLs39X/aUfwRclRaXvNeXNBTLZdnZdHhhteqbYFS2jZTRD79tKeFeBn7SGXNpi", crossorigin="anonymous"] {}
                script[src=format!("{}/{}/{}", infos.root_path.display(), infos.krate_name, SEARCH_INDEX_JS)] {}
                script[src=format!("{}/{}", infos.root_path.display(), SEARCH_JS)] {}
            }
        }
    }

    ItemPage<
        'a,
        Definition: markup::Render,
        ItemPath: markup::Render,
        Deprecation: markup::Render,
        Portability: markup::Render,
        Content: markup::Render
    > (
        item_type: &'a str,
        item_name: &'a str,
        item_path: ItemPath,
        toc: &'a Vec<TocSection<'a>>,
        item_definition: Option<Definition>,
        item_deprecation: Option<Deprecation>,
        item_portability: Option<Portability>,
        item_doc: Option<MarkdownWithToc<'a, 'a, 'a>>,
        content: Option<Content>
    ) {
        div[class="rd-main"] {
            div[class="rd-intro"] {
                h1[id="item-title", class="rd-anchor item-title"] {
                    @item_type
                    " "
                    @item_path
                }
                @if item_definition.is_some() {
                    pre[id="item-definition", class="rd-anchor item-definition"] {
                        code {
                            @item_definition
                        }
                    }
                }
                @item_deprecation
                @item_portability
                @if item_doc.is_some() {
                    details[id="item-documentation", class="rd-anchor item-documentation", open=""] {
                        summary {
                            "Documentation"
                        }
                        div[class = "mt-2"] {
                            @item_doc
                        }
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
                                    a[href="#item-documentation", class="d-inline-block align-items-center rounded"] { strong { "Documentation" } }
                                } else {
                                    a[class="rd-btn-toc d-inline-block align-items-center rounded bi bi-caret-right-fill", href="#item-documentation", "data-bs-toggle"="collapse", "data-bs-target"="#toc-documentation", "aria-expanded"="true", "aria-current"="true"] { strong { "Documentation" } }
                                    ul[id="toc-documentation", class="collapse show"] {
                                        @for (level, ref name, destination) in item_doc.4.borrow_mut().iter() {
                                            @if *level == 1 {
                                                li {
                                                    a[href=destination.with_pound(), class="d-inline-block align-items-center rounded"] {
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
                            @if !section_items.is_empty() {
                                li {
                                    a[class="rd-btn-toc d-inline-block align-items-center rounded bi bi-caret-right-fill", href=format!("#{}", section_id), "data-bs-toggle"="collapse", "data-bs-target"=format!("#toc-{}", section_id), "aria-expanded"="true", "aria-current"="true"] { strong { @section_name } }
                                    ul[id=format!("toc-{}", section_id), class="collapse show"] {
                                        @for (ref name, destination) in section_items {
                                            li {
                                                a[href=destination, class="d-inline-block align-items-center rounded"] {
                                                    @name.deref()
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

    DeprecationNotice<
        'deprecation,
    > (since: &'deprecation Option<String>, note : &'deprecation Option<String>) {
        div[class="alert alert-warning", role="alert"] {
            i[class="bi bi-exclamation-triangle me-2"] {}
            "Deprecated"
            @if let Some(since) = &since {
                " since "
                @since
            }
            @if let Some(note) = &note {
                ": "
                @note
            }
        }
    }

    PortabilityNotice<
        'portability
    > (message: &'portability str, portability: &'portability str) {
        div[class="alert alert-primary", role="alert"] {
            i[class="bi bi-info-circle me-2"] {}
            @message
            code { @portability }
        }
    }

    ModuleSectionItem<
        Item: markup::Render,
        Summary: markup::Render,
        Portability: markup::Render,
    > (name: Item, summary: Summary, deprecated: bool, portability: Option<Portability>) {
        div {
            p {
                @name
                @if *deprecated {
                    span[class="badge bg-warning text-wrap text-dark ms-1"] { "Deprecated" }
                }
                @if portability.is_some() {
                    span[class="badge bg-primary text-wrap ms-1"] { @portability }
                }
            }
        }
        div { @summary }
    }

    ModuleSection<
        'name,
        Item: markup::Render,
    > (name: &'name str, id: &'name str, items: &'name Vec<Item>) {
        @if !items.is_empty() {
            section {
                h3[id=id, class="border-bottom rd-anchor"] { @name }
                div[class = "item-table"] {
                    @for item in *items {
                        @item
                    }
                }
            }
        }
    }

    GeneralSection<
        'name,
        Item: markup::Render,
    > (name: &'name str, id: &'name str, items: &'name Vec<Item>) {
        @if !items.is_empty() {
            section {
                h3[id=id, class="border-bottom rd-anchor"] { @name }
                @for item in *items {
                    @item
                }
            }
        }
    }

    InlineCode<Code: markup::Render> (code: Code) {
        code[class="inline-code"] { @code }
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
        @InlineCode { code }
    }

    CodeEnchanted<
        Code: markup::Render,
        Documentation: markup::Render,
        Deprecation: markup::Render,
        Id: markup::Render,
    > (code: Code, doc: Option<Documentation>, deprecation: Option<Deprecation>, id: Option<Id>, open: bool, source_href: Option<String>) {
        div[id=id, class="mt-2 mb-2 rd-anchor"] {
            @if doc.is_some() {
                details[open=open] {
                    summary {
                        @InlineCodeWithSource { code, source_href }
                    }
                    @deprecation
                    div[class="mt-2 item-documentation"] { @doc }
                }
            } else {
                @InlineCodeWithSource { code, source_href }
                @deprecation
            }
        }
    }

    CodeEnchantedWithExtras<
        Code: markup::Render,
        Documentation: markup::Render,
        Deprecation: markup::Render,
        Id: markup::Render,
        Extra: markup::Render,
    > (code: Code, doc: Option<Documentation>, deprecation: Option<Deprecation>, extras: Vec<Extra>, id: Option<Id>, open: bool, source_href: Option<String>) {
        div[id=id, class="mt-2 mb-2 rd-anchor"] {
            @if doc.is_some() || !extras.is_empty() {
                details[open=open] {
                    summary {
                        @InlineCodeWithSource { code, source_href }
                    }
                    @deprecation
                    div[class="mt-2 item-documentation"] { @doc }
                    div[style = "padding-left:1.5rem;"] {
                        @for extra in extras {
                            @extra
                        }
                    }
                }
            } else {
                @InlineCodeWithSource { code, source_href }
                @deprecation
            }
        }
    }

    VariantEnchanted<
        Id: markup::Render,
        Definition: markup::Render,
        Documentation: markup::Render,
        Deprecation: markup::Render,
    > (id: Id, def: Definition, doc: Option<Documentation>, deprecation: Option<Deprecation>) {
        div[id=id, class="rd-anchor"] {
            @InlineCode { code: def }
            @deprecation
            @if doc.is_some() {
                div[class="item-documentation"] { @doc }
            }
        }
    }

    VariantEnchantedWithExtras<
        Id: markup::Render,
        Definition: markup::Render,
        Documentation: markup::Render,
        Deprecation: markup::Render,
        Extra: markup::Render,
    > (id: Id, def: Definition, doc: Option<Documentation>, deprecation: Option<Deprecation>, extras: Option<Vec<Extra>>) {
        div[id=id, class="mt-2 mb-2 rd-anchor"] {
            @InlineCode { code: def }
            @deprecation
            @if let Some(extras) = extras {
                div[style = "padding-left:1.5rem;"] {
                    @for extra in extras {
                        @extra
                    }
                }
            }
            @if doc.is_some() {
                div[class="mt-2 item-documentation"] { @doc }
            }
        }
    }

    StructUnionEnumContent<
        'title,
        Variant: markup::Render,
        Traits: markup::Render
    > (title: &'title str, variants: Vec<Variant>, traits: Traits) {
        @if !variants.is_empty() {
            section {
                h2[class="border-bottom pb-1 rd-anchor", id=VARIANTS_ID] { @title }
                @for variant in variants {
                    @variant
                }
            }
        }
        @traits
    }

    ModulePageContent<
        ImportItem: markup::Render,
        ModuleItem: markup::Render,
        UnionItem: markup::Render,
        StructItem: markup::Render,
        EnumItem: markup::Render,
        FunctionItem: markup::Render,
        TraitItem: markup::Render,
        TraitAliasItem: markup::Render,
        TypedefItem: markup::Render,
        ConstantItem: markup::Render,
        MacroItem: markup::Render,
        ProcMacroItem: markup::Render,
    > (
        imports: Vec<ImportItem>,
        modules: Vec<ModuleItem>,
        unions: Vec<UnionItem>,
        structs: Vec<StructItem>,
        enums: Vec<EnumItem>,
        functions: Vec<FunctionItem>,
        traits: Vec<TraitItem>,
        trait_alias: Vec<TraitAliasItem>,
        typedefs: Vec<TypedefItem>,
        constants: Vec<ConstantItem>,
        macros: Vec<MacroItem>,
        proc_macros: Vec<ProcMacroItem>,
    ) {
        @ModuleSection { name: IMPORTS, id: IMPORTS_ID, items: imports }
        @ModuleSection { name: MODULES, id: MODULES_ID, items: modules }
        @ModuleSection { name: MACROS, id: MACROS_ID, items: macros }
        @ModuleSection { name: PROC_MACROS, id: PROC_MACROS_ID, items: proc_macros }
        @ModuleSection { name: UNIONS, id: UNIONS_ID, items: unions }
        @ModuleSection { name: STRUCTS, id: STRUCTS_ID, items: structs }
        @ModuleSection { name: ENUMS, id: ENUMS_ID, items: enums }
        @ModuleSection { name: FUNCTIONS, id: FUNCTIONS_ID, items: functions }
        @ModuleSection { name: TRAITS, id: TRAITS_ID, items: traits }
        @ModuleSection { name: TRAIT_ALIAS, id: TRAIT_ALIAS_ID, items: trait_alias }
        @ModuleSection { name: TYPEDEFS, id: TYPEDEFS_ID, items: typedefs }
        @ModuleSection { name: CONSTANTS, id: CONSTANTS_ID, items: constants }
    }

    TraitPageContent<Code: markup::Render, Trait: markup::Render>(
        associated_types: Vec<Code>,
        associated_consts: Vec<Code>,
        required_methods: Vec<Code>,
        provided_methods: Vec<Code>,
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
                            a[class="nav-link p-2", href="#themes", title="Toggle themes", onclick="darkmode.toggleDarkMode()"] {
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
        nav[class="rd-subnavbar py-2 border-bottom shadow-sm", "aria-label"="Secondary navigation"] {
            div[class="container-xxl d-flex align-items-md-center"] {
                form[class="rd-search position-relative", id="rd-search-form"] {
                    span[class="w-100", style="position: relative; display: inline-block; direction: ltr;"] {
                        input[type="search", class="form-control ds-input", id="rd-search-input", placeholder=format!("Search in {}...", krate_name), "aria-label"="Search docs for...", autocomplete="off", spellcheck="false", role="combobox", "aria-autocomplete"="list", "aria-expanded"="false", "aria-owns"="rd-search-menu", style="position: relative; vertical-align: top;", dir="auto"];
                        span[class="ds-dropdown-menu", style="position: absolute; top: 100%; z-index: 100; display: none; left: 0px; right: 0px;", role="listbox", id="rd-search-menu"] {
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
