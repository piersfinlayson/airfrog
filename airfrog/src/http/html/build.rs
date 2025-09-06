//! HTML Builder

// Copyright (C) 2025 Piers Finlayson <piers@piers.rocks>
//
// MIT License

#![allow(dead_code)]

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::marker::PhantomData;

// State markers for type safety
pub struct Open;
pub struct Closed;

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub struct HtmlBuilder {
    content: String,
}

impl HtmlBuilder {
    pub fn new() -> Self {
        Self {
            content: String::new(),
        }
    }

    pub fn h1(mut self, text: &str) -> Self {
        self.content
            .push_str(&format!("<h1>{}</h1>", escape_html(text)));
        self
    }

    pub fn h2(mut self, text: &str) -> Self {
        self.content
            .push_str(&format!("<h2>{}</h2>", escape_html(text)));
        self
    }

    pub fn div(self) -> ElementBuilder {
        ElementBuilder::new("div", self)
    }

    pub fn with_table<F>(mut self, class: Option<&str>, f: F) -> Self
    where
        F: FnOnce(TableBuilder<Open>) -> TableBuilder<Open>,
    {
        let table = TableBuilder::new(class);
        let completed_table = f(table);
        let closed_table: TableBuilder<Closed> = completed_table.into();
        self.content.push_str(&closed_table.finish());
        self
    }

    pub fn script_src(mut self, src: &str) -> Self {
        self.content
            .push_str(&format!("<script src=\"{}\"></script>", escape_html(src)));
        self
    }

    pub fn script(mut self, code: &str) -> Self {
        // Script content should not be HTML escaped as it's JavaScript
        self.content.push_str(&format!("<script>{code}</script>"));
        self
    }

    pub fn link(mut self, href: &str, text: &str) -> Self {
        self.content.push_str(&format!(
            "<a href=\"{}\">{}</a>",
            escape_html(href),
            escape_html(text)
        ));
        self
    }

    pub fn button(mut self, text: &str, onclick: &str) -> Self {
        self.content.push_str(&format!(
            "<button onclick=\"{}\">{}</button>",
            escape_html(onclick),
            escape_html(text)
        ));
        self
    }

    pub fn button_plain(mut self, text: &str) -> Self {
        self.content
            .push_str(&format!("<button>{}</button>", escape_html(text)));
        self
    }

    pub fn br(mut self) -> Self {
        self.content.push_str("<br/>");
        self
    }

    pub fn text(mut self, text: &str) -> Self {
        self.content.push_str(&escape_html(text));
        self
    }

    pub fn build(self) -> String {
        self.content
    }
}

impl Default for HtmlBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ElementBuilder {
    tag: &'static str,
    builder: HtmlBuilder,
    attributes: Vec<(&'static str, String)>,
}

impl ElementBuilder {
    fn new(tag: &'static str, builder: HtmlBuilder) -> Self {
        Self {
            tag,
            builder,
            attributes: Vec::new(),
        }
    }

    pub fn class(mut self, class: &str) -> Self {
        self.attributes.push(("class", class.to_string()));
        self
    }

    pub fn id(mut self, id: &str) -> Self {
        self.attributes.push(("id", id.to_string()));
        self
    }

    pub fn child<F>(mut self, f: F) -> HtmlBuilder
    where
        F: FnOnce(HtmlBuilder) -> HtmlBuilder,
    {
        // Open tag with attributes
        let attrs = self
            .attributes
            .iter()
            .map(|(k, v)| format!(" {}=\"{}\"", k, escape_html(v)))
            .collect::<String>();
        self.builder
            .content
            .push_str(&format!("<{}{}>", self.tag, attrs));

        // Build children
        let child_builder = HtmlBuilder::new();
        let child_result = f(child_builder);
        self.builder.content.push_str(&child_result.content);

        // Close tag
        self.builder.content.push_str(&format!("</{}>", self.tag));
        self.builder
    }

    pub fn text(mut self, text: &str) -> HtmlBuilder {
        let attrs = self
            .attributes
            .iter()
            .map(|(k, v)| format!(" {}=\"{}\"", k, escape_html(v)))
            .collect::<String>();
        self.builder.content.push_str(&format!(
            "<{}{}>{}</{}>",
            self.tag,
            attrs,
            escape_html(text),
            self.tag
        ));
        self.builder
    }
}

pub struct TableBuilder<State = Open> {
    content: String,
    _state: PhantomData<State>,
}

impl TableBuilder<Open> {
    fn new(class: Option<&str>) -> Self {
        let class_attr = class.map_or(String::new(), |c| format!(" class=\"{}\"", escape_html(c)));
        let mut content = String::new();
        content.push_str(&format!("<table{class_attr}>"));

        Self {
            content,
            _state: PhantomData,
        }
    }

    pub fn row<F>(mut self, f: F) -> Self
    where
        F: FnOnce(RowBuilder<Open>) -> RowBuilder<Open>,
    {
        let row_builder = RowBuilder::new();
        let completed_row = f(row_builder);
        let closed_row: RowBuilder<Closed> = completed_row.into();
        self.content.push_str(&closed_row.finish());
        self
    }

    pub fn close(self) -> TableBuilder<Closed> {
        TableBuilder {
            content: self.content,
            _state: PhantomData,
        }
    }
}

impl TableBuilder<Closed> {
    pub fn finish(mut self) -> String {
        self.content.push_str("</table>");
        self.content
    }
}

// Auto-close table if user forgets
impl From<TableBuilder<Open>> for TableBuilder<Closed> {
    fn from(open_table: TableBuilder<Open>) -> Self {
        open_table.close()
    }
}

pub struct RowBuilder<State = Open> {
    content: String,
    pending_width: Option<String>,
    _state: PhantomData<State>,
}

impl RowBuilder<Open> {
    fn new() -> Self {
        let mut content = String::new();
        content.push_str("<tr>");
        Self {
            content,
            pending_width: None,
            _state: PhantomData,
        }
    }

    pub fn with_width(mut self, width: &str) -> Self {
        self.pending_width = Some(width.to_string());
        self
    }

    pub fn cell(mut self, content: &str) -> Self {
        let width_attr = self
            .pending_width
            .map(|w| format!(" style=\"width: {};\"", escape_html(&w)))
            .unwrap_or_default();

        self.content
            .push_str(&format!("<td{}>{}</td>", width_attr, escape_html(content)));
        self.pending_width = None; // Width only applies to next cell
        self
    }

    pub fn label_cell(mut self, content: &str) -> Self {
        let width_attr = self
            .pending_width
            .map(|w| format!(" style=\"width: {};\"", escape_html(&w)))
            .unwrap_or_default();

        self.content.push_str(&format!(
            "<td class=\"label-col\"{}><strong>{}</strong></td>",
            width_attr,
            escape_html(content)
        ));
        self.pending_width = None; // Width only applies to next cell
        self
    }

    pub fn link_cell(mut self, href: &str, text: &str) -> Self {
        let width_attr = self
            .pending_width
            .map(|w| format!(" style=\"width: {};\"", escape_html(&w)))
            .unwrap_or_default();

        self.content.push_str(&format!(
            "<td{}><a href=\"{}\">{}</a></td>",
            width_attr,
            escape_html(href),
            escape_html(text)
        ));
        self.pending_width = None; // Width only applies to next cell
        self
    }

    pub fn close(self) -> RowBuilder<Closed> {
        RowBuilder {
            content: self.content,
            pending_width: self.pending_width,
            _state: PhantomData,
        }
    }
}

impl RowBuilder<Closed> {
    fn finish(mut self) -> String {
        self.content.push_str("</tr>");
        self.content
    }
}

// Auto-close row if user forgets
impl From<RowBuilder<Open>> for RowBuilder<Closed> {
    fn from(open_row: RowBuilder<Open>) -> Self {
        open_row.close()
    }
}
