//! Derive macros for the TORM library.
//!
//! The main macro is [`Model`](macro@Model), which generates the verbose
//! boilerplate for implementing `torm::orm::model::Model` from a plain
//! struct definition.
//!
//! # Example
//!
//! ```ignore
//! use torm::{Model, Timestamps};
//!
//! #[derive(Debug, Clone, Model)]
//! #[model(table_name = "users", primary_key = "id")]
//! pub struct User {
//!     pub id: i64,
//!     pub name: String,
//!     pub created_at: Option<chrono::DateTime<chrono::Utc>>,
//!     pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
//!     pub timestamps: Timestamps,
//! }
//! ```

extern crate proc_macro;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{
    parse_macro_input, Attribute, Data, DeriveInput, Expr, Fields, Lit, Meta, Type,
    punctuated::Punctuated, spanned::Spanned, token::Comma,
};

/// The role a struct field plays in the generated `Model` impl.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    /// The primary-key field (`id` by default).
    PrimaryKey,
    /// A `Timestamps` struct field backing the timestamp accessors.
    Timestamps,
    /// A standalone `created_at`/`updated_at`/`deleted_at` timestamp field.
    TimestampField,
    /// A normal, persistable field (goes into `columns()` / `from_row()`).
    Persist,
    /// Excluded from persistence (`#[model(skip)]` or unsupported type).
    Skip,
}

/// Decorated struct field.
struct FieldInfo {
    ident: syn::Ident,
    column: Option<String>,
    kind: FieldKind,
    ty: Type,
    /// Whether the type is `Option<...>`.
    optional: bool,
}

impl FieldInfo {
    fn column_name(&self) -> String {
        self.column
            .clone()
            .unwrap_or_else(|| self.ident.to_string())
    }
}

/// Container-level `#[model(...)]` configuration.
struct ModelConfig {
    table_name: String,
    primary_key: String,
}

impl ModelConfig {
    fn parse(attrs: &[Attribute]) -> syn::Result<Self> {
        let mut table_name: Option<String> = None;
        let mut primary_key: Option<String> = None;

        for attr in attrs {
            if !attr.path().is_ident("model") {
                continue;
            }
            let Meta::List(list) = &attr.meta else { continue };
            let nested = list.parse_args_with(Punctuated::<Meta, Comma>::parse_terminated)?;
            for item in nested {
                match item {
                    Meta::NameValue(nv) if nv.path.is_ident("table_name") => {
                        table_name = Some(expr_to_string(&nv.value)?);
                    }
                    Meta::NameValue(nv) if nv.path.is_ident("primary_key") => {
                        primary_key = Some(expr_to_string(&nv.value)?);
                    }
                    _ => {}
                }
            }
        }

        let table_name = table_name.ok_or_else(|| {
            syn::Error::new(
                Span::call_site(),
                "#[model(table_name = \"...\")] is required on #[derive(Model)] structs",
            )
        })?;

        Ok(Self {
            table_name,
            primary_key: primary_key.unwrap_or_else(|| "id".to_string()),
        })
    }
}

fn expr_to_string(expr: &Expr) -> syn::Result<String> {
    match expr {
        Expr::Lit(lit) => match &lit.lit {
            Lit::Str(s) => Ok(s.value()),
            _ => Err(syn::Error::new_spanned(lit, "expected a string literal")),
        },
        _ => Err(syn::Error::new_spanned(expr, "expected a string literal")),
    }
}

/// Collect and decorate all fields of the input struct.
fn collect_fields(input: &DeriveInput, pk_name: &str) -> syn::Result<Vec<FieldInfo>> {
    let data = match &input.data {
        Data::Struct(s) => s,
        Data::Enum(_) | Data::Union(_) => {
            return Err(syn::Error::new(
                input.ident.span(),
                "#[derive(Model)] is only supported on structs",
            ))
        }
    };

    let named = match &data.fields {
        Fields::Named(named) => &named.named,
        Fields::Unnamed(_) | Fields::Unit => {
            return Err(syn::Error::new(
                input.ident.span(),
                "#[derive(Model)] requires named struct fields",
            ))
        }
    };

    let mut out = Vec::new();
    for field in named {
        let ident = field
            .ident
            .clone()
            .ok_or_else(|| syn::Error::new(field.span(), "named field expected"))?;
        let ty = field.ty.clone();
        let optional = field_type_is_option(&ty);

        let mut skip = false;
        let mut column: Option<String> = None;
        for attr in &field.attrs {
            if !attr.path().is_ident("model") {
                continue;
            }
            let Meta::List(list) = &attr.meta else { continue };
            let nested = list.parse_args_with(Punctuated::<Meta, Comma>::parse_terminated)?;
            for item in nested {
                match item {
                    Meta::Path(path) if path.is_ident("skip") => skip = true,
                    Meta::NameValue(nv) if nv.path.is_ident("column") => {
                        column = Some(expr_to_string(&nv.value)?);
                    }
                    _ => {}
                }
            }
        }

        let kind = if skip {
            FieldKind::Skip
        } else if field_type_is_timestamps(&ty) {
            FieldKind::Timestamps
        } else if ident.to_string() == pk_name {
            FieldKind::PrimaryKey
        } else if is_standalone_timestamp_field(&ident, &ty) {
            FieldKind::TimestampField
        } else if type_tag(&ty).is_some() {
            FieldKind::Persist
        } else {
            // Unsupported / association types are silently excluded.
            FieldKind::Skip
        };

        out.push(FieldInfo {
            ident,
            column,
            kind,
            ty,
            optional,
        });
    }

    Ok(out)
}

fn field_type_is_timestamps(ty: &Type) -> bool {
    let Type::Path(tp) = ty else { return false };
    matches!(tp.path.segments.last(), Some(s) if s.ident == "Timestamps")
}

fn field_type_is_option(ty: &Type) -> bool {
    let Type::Path(tp) = ty else { return false };
    matches!(tp.path.segments.last(), Some(s) if s.ident == "Option")
}

/// True if this is a standalone timestamp field like
/// `created_at: Option<DateTime<Utc>>`.
fn is_standalone_timestamp_field(ident: &syn::Ident, ty: &Type) -> bool {
    let name = ident.to_string();
    let valid_name = matches!(name.as_str(), "created_at" | "updated_at" | "deleted_at");
    valid_name && type_tag(ty) == Some("datetime")
}

/// The innermost type when a type is `Option<...>` (or itself).
fn inner_type(ty: &Type) -> &Type {
    if let Type::Path(tp) = ty {
        if let Some(s) = tp.path.segments.last() {
            if s.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &s.arguments {
                    if let Some(syn::GenericArgument::Type(inner)) = args.args.first() {
                        return inner;
                    }
                }
            }
        }
    }
    ty
}

/// A simple type tag for the persistable set.
fn type_tag(ty: &Type) -> Option<&'static str> {
    let ty = inner_type(ty);
    let Type::Path(tp) = ty else { return None };
    let last = tp.path.segments.last()?;
    let name = last.ident.to_string();
    let tag = match name.as_str() {
        "String" => "string",
        "bool" => "bool",
        "i8" => "i8",
        "i16" => "i16",
        "i32" => "i32",
        "i64" => "i64",
        "f32" => "f32",
        "f64" => "f64",
        "DateTime" => "datetime",
        "Uuid" => "uuid",
        "Vec" => {
            // Only `Vec<u8>` maps to bytes; other Vec<T> are treated as
            // unsupported (association fields) and auto-skipped.
            let is_u8 = matches!(
                &last.arguments,
                syn::PathArguments::AngleBracketed(args)
                    if args.args.len() == 1
                        && matches!(
                            args.args.first(),
                            Some(syn::GenericArgument::Type(Type::Path(tp)))
                                if tp.path.is_ident("u8")
                        )
            );
            if is_u8 {
                "bytes"
            } else {
                return None;
            }
        }
        _ => return None,
    };
    Some(tag)
}

/// Generate the `columns()` vec entries for the persist fields.
fn gen_columns_entries(fields: &[&FieldInfo]) -> Vec<proc_macro2::TokenStream> {
    fields
        .iter()
        .filter_map(|f| {
            let col = f.column_name();
            let fname = &f.ident;
            let tag = type_tag(&f.ty)?;
            let expr = match tag {
                "string" => {
                    if f.optional {
                        quote! { match &self.#fname { Some(v) => SqlValue::String(v.clone()), None => SqlValue::Null } }
                    } else {
                        quote! { SqlValue::String(self.#fname.clone()) }
                    }
                }
                "bool" => {
                    if f.optional {
                        quote! { match self.#fname { Some(v) => SqlValue::Bool(v), None => SqlValue::Null } }
                    } else {
                        quote! { SqlValue::Bool(self.#fname) }
                    }
                }
                "i8" => {
                    if f.optional {
                        quote! { match self.#fname { Some(v) => SqlValue::I8(v), None => SqlValue::Null } }
                    } else {
                        quote! { SqlValue::I8(self.#fname) }
                    }
                }
                "i16" => {
                    if f.optional {
                        quote! { match self.#fname { Some(v) => SqlValue::I16(v), None => SqlValue::Null } }
                    } else {
                        quote! { SqlValue::I16(self.#fname) }
                    }
                }
                "i32" => {
                    if f.optional {
                        quote! { match self.#fname { Some(v) => SqlValue::I32(v), None => SqlValue::Null } }
                    } else {
                        quote! { SqlValue::I32(self.#fname) }
                    }
                }
                "i64" => {
                    if f.optional {
                        quote! { match self.#fname { Some(v) => SqlValue::I64(v), None => SqlValue::Null } }
                    } else {
                        quote! { SqlValue::I64(self.#fname) }
                    }
                }
                "f32" => {
                    if f.optional {
                        quote! { match self.#fname { Some(v) => SqlValue::F32(v), None => SqlValue::Null } }
                    } else {
                        quote! { SqlValue::F32(self.#fname) }
                    }
                }
                "f64" => {
                    if f.optional {
                        quote! { match self.#fname { Some(v) => SqlValue::F64(v), None => SqlValue::Null } }
                    } else {
                        quote! { SqlValue::F64(self.#fname) }
                    }
                }
                "datetime" => {
                    if f.optional {
                        quote! { match self.#fname { Some(v) => SqlValue::DateTime(v), None => SqlValue::Null } }
                    } else {
                        quote! { SqlValue::DateTime(self.#fname) }
                    }
                }
                "uuid" => {
                    if f.optional {
                        quote! { match &self.#fname { Some(v) => SqlValue::String(v.to_string()), None => SqlValue::Null } }
                    } else {
                        quote! { SqlValue::String(self.#fname.to_string()) }
                    }
                }
                "bytes" => {
                    if f.optional {
                        quote! { match &self.#fname { Some(v) => SqlValue::Bytes(v.clone()), None => SqlValue::Null } }
                    } else {
                        quote! { SqlValue::Bytes(self.#fname.clone()) }
                    }
                }
                _ => return None,
            };
            Some(quote! { (#col, #expr) })
        })
        .collect()
}

/// Generate a `from_row` field initializer expression yielding the field value.
fn gen_from_row_expr(f: &FieldInfo) -> Option<proc_macro2::TokenStream> {
    let col = f.column_name();
    let tag = type_tag(&f.ty)?;
    let inner = match tag {
        "string" => Some(quote! {
            match row.get(#col)? {
                SqlValue::String(s) => s.clone(),
                SqlValue::Json(s) => s.clone(),
                SqlValue::I64(i) => i.to_string(),
                SqlValue::I32(i) => i.to_string(),
                SqlValue::I16(i) => i.to_string(),
                SqlValue::I8(i) => i.to_string(),
                SqlValue::Bool(b) => b.to_string(),
                _ => return None,
            }
        }),
        "bool" => Some(quote! {
            match row.get(#col)? {
                SqlValue::Bool(v) => *v,
                SqlValue::I32(1) => true,
                SqlValue::I64(1) => true,
                SqlValue::I32(0) => false,
                SqlValue::I64(0) => false,
                _ => return None,
            }
        }),
        "i8" => Some(quote! {
            match row.get(#col)? {
                SqlValue::I8(v) => *v,
                SqlValue::I16(v) => *v as i8,
                SqlValue::I32(v) => *v as i8,
                SqlValue::I64(v) => *v as i8,
                _ => return None,
            }
        }),
        "i16" => Some(quote! {
            match row.get(#col)? {
                SqlValue::I16(v) => *v,
                SqlValue::I8(v) => *v as i16,
                SqlValue::I32(v) => *v as i16,
                SqlValue::I64(v) => *v as i16,
                _ => return None,
            }
        }),
        "i32" => Some(quote! {
            match row.get(#col)? {
                SqlValue::I32(v) => *v,
                SqlValue::I8(v) => *v as i32,
                SqlValue::I16(v) => *v as i32,
                SqlValue::I64(v) => *v as i32,
                _ => return None,
            }
        }),
        "i64" => Some(quote! {
            match row.get(#col)? {
                SqlValue::I64(v) => *v,
                SqlValue::I32(v) => *v as i64,
                SqlValue::I16(v) => *v as i64,
                SqlValue::I8(v) => *v as i64,
                _ => return None,
            }
        }),
        "f32" => Some(quote! {
            match row.get(#col)? {
                SqlValue::F32(v) => *v,
                SqlValue::F64(v) => *v as f32,
                _ => return None,
            }
        }),
        "f64" => Some(quote! {
            match row.get(#col)? {
                SqlValue::F64(v) => *v,
                SqlValue::F32(v) => *v as f64,
                _ => return None,
            }
        }),
        "datetime" => Some(quote! {
            match row.get(#col)? {
                SqlValue::DateTime(v) => *v,
                // SQLite stores datetimes as TEXT; parse the common format.
                SqlValue::String(s) => {
                    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                        .map(|dt| dt.and_utc())
                        .ok()?
                }
                _ => return None,
            }
        }),
        "uuid" => Some(quote! {
            Uuid::parse_str(row.get(#col)?.as_str()?).ok()?
        }),
        "bytes" => Some(quote! {
            match row.get(#col)? {
                SqlValue::Bytes(v) => v.clone(),
                _ => return None,
            }
        }),
        _ => None,
    }?;

    if f.optional {
        Some(quote! {
            match row.get(#col) {
                Some(SqlValue::Null) | None => None,
                _ => Some(#inner),
            }
        })
    } else {
        Some(inner)
    }
}

/// Build the complete `impl Model for ...` block.
fn build_model_impl(
    input: &DeriveInput,
    config: &ModelConfig,
    fields: &[FieldInfo],
) -> proc_macro2::TokenStream {
    let ident = &input.ident;
    let table_name = &config.table_name;

    let pk = fields.iter().find(|f| f.kind == FieldKind::PrimaryKey);
    let ts = fields.iter().find(|f| f.kind == FieldKind::Timestamps);
    let ts_field_created = fields
        .iter()
        .find(|f| f.kind == FieldKind::TimestampField && f.ident == "created_at");
    let ts_field_updated = fields
        .iter()
        .find(|f| f.kind == FieldKind::TimestampField && f.ident == "updated_at");
    let ts_field_deleted = fields
        .iter()
        .find(|f| f.kind == FieldKind::TimestampField && f.ident == "deleted_at");
    let persist: Vec<&FieldInfo> = fields
        .iter()
        .filter(|f| f.kind == FieldKind::Persist)
        .collect();

    // --- id / set_id ---
    let (id_fn, set_id_fn) = match pk {
        Some(pk) => {
            let pk_ident = &pk.ident;
            let pk_ty = inner_type(&pk.ty);
            let pk_tag = type_tag(pk_ty).unwrap_or("");
            let (id_expr, set_expr) = match pk_tag {
                "string" => (
                    quote! {
                        if self.#pk_ident.is_empty() { None } else { Some(self.#pk_ident.clone()) }
                    },
                    quote! { self.#pk_ident = id; },
                ),
                "uuid" => (
                    quote! {
                        if self.#pk_ident.is_nil() { None } else { Some(self.#pk_ident.to_string()) }
                    },
                    quote! { self.#pk_ident = Uuid::parse_str(&id).unwrap_or_else(|_| Uuid::nil()); },
                ),
                "i64" => (
                    quote! {
                        if self.#pk_ident > 0 { Some(self.#pk_ident.to_string()) } else { None }
                    },
                    quote! { self.#pk_ident = id.parse().unwrap_or(0); },
                ),
                "i32" => (
                    quote! {
                        if self.#pk_ident > 0 { Some(self.#pk_ident.to_string()) } else { None }
                    },
                    quote! { self.#pk_ident = id.parse().unwrap_or(0); },
                ),
                "i16" => (
                    quote! {
                        if self.#pk_ident > 0 { Some(self.#pk_ident.to_string()) } else { None }
                    },
                    quote! { self.#pk_ident = id.parse().unwrap_or(0); },
                ),
                "i8" => (
                    quote! {
                        if self.#pk_ident > 0 { Some(self.#pk_ident.to_string()) } else { None }
                    },
                    quote! { self.#pk_ident = id.parse().unwrap_or(0); },
                ),
                // Fallback: ToString + From<String>.
                _ => (
                    quote! {
                        let s = self.#pk_ident.to_string();
                        if s.is_empty() { None } else { Some(s) }
                    },
                    quote! { self.#pk_ident = id.into(); },
                ),
            };
            (
                quote! { fn id(&self) -> Option<String> { #id_expr } },
                quote! { fn set_id(&mut self, id: String) { #set_expr } },
            )
        }
        None => (
            quote! {
                fn id(&self) -> Option<String> { None }
            },
            quote! {
                fn set_id(&mut self, _id: String) {}
            },
        ),
    };

    // --- timestamps accessors ---
    // Prefer a `Timestamps` struct field; otherwise fall back to standalone
    // created_at/updated_at/deleted_at fields.
    let ts_impl = if let Some(ts) = ts {
        let t = &ts.ident;
        quote! {
            fn created_at(&self) -> Option<DateTime<Utc>> { self.#t.created_at }
            fn updated_at(&self) -> Option<DateTime<Utc>> { self.#t.updated_at }
            fn deleted_at(&self) -> Option<DateTime<Utc>> { self.#t.deleted_at }
            fn set_created_at(&mut self, timestamp: DateTime<Utc>) { self.#t.created_at = Some(timestamp); }
            fn set_updated_at(&mut self, timestamp: DateTime<Utc>) { self.#t.updated_at = Some(timestamp); }
            fn set_deleted_at(&mut self, timestamp: Option<DateTime<Utc>>) { self.#t.deleted_at = timestamp; }
        }
    } else {
        let created = ts_field_created.map(|f| &f.ident);
        let updated = ts_field_updated.map(|f| &f.ident);
        let deleted = ts_field_deleted.map(|f| &f.ident);
        let (created_get, created_set) = match created {
            Some(ci) => (
                quote! { self.#ci },
                quote! { self.#ci = Some(timestamp); },
            ),
            None => (quote! { None }, quote! {}),
        };
        let (updated_get, updated_set) = match updated {
            Some(ui) => (
                quote! { self.#ui },
                quote! { self.#ui = Some(timestamp); },
            ),
            None => (quote! { None }, quote! {}),
        };
        let (deleted_get, deleted_set) = match deleted {
            Some(di) => (
                quote! { self.#di },
                quote! { self.#di = timestamp; },
            ),
            None => (quote! { None }, quote! {}),
        };
        quote! {
            fn created_at(&self) -> Option<DateTime<Utc>> { #created_get }
            fn updated_at(&self) -> Option<DateTime<Utc>> { #updated_get }
            fn deleted_at(&self) -> Option<DateTime<Utc>> { #deleted_get }
            fn set_created_at(&mut self, timestamp: DateTime<Utc>) { #created_set }
            fn set_updated_at(&mut self, timestamp: DateTime<Utc>) { #updated_set }
            fn set_deleted_at(&mut self, timestamp: Option<DateTime<Utc>>) { #deleted_set }
        }
    };

    // --- columns() ---
    let column_entries = gen_columns_entries(&persist);

    // --- from_row() ---
    let mut literals: Vec<proc_macro2::TokenStream> = Vec::new();
    for f in fields {
        let fident = &f.ident;
        match f.kind {
            FieldKind::Skip => {
                literals.push(quote! { #fident: Default::default() });
            }
            FieldKind::Timestamps => {
                literals.push(quote! {
                    #fident: {
                        let mut ts = Timestamps::new();
                        ts.created_at = row.get("created_at").and_then(|v| match v {
                            SqlValue::DateTime(dt) => Some(*dt),
                            _ => None,
                        });
                        ts.updated_at = row.get("updated_at").and_then(|v| match v {
                            SqlValue::DateTime(dt) => Some(*dt),
                            _ => None,
                        });
                        ts.deleted_at = row.get("deleted_at").and_then(|v| match v {
                            SqlValue::DateTime(dt) => Some(*dt),
                            _ => None,
                        });
                        ts
                    }
                });
            }
            FieldKind::PrimaryKey | FieldKind::Persist | FieldKind::TimestampField => {
                if let Some(expr) = gen_from_row_expr(f) {
                    literals.push(quote! { #fident: #expr });
                } else {
                    literals.push(quote! { #fident: Default::default() });
                }
            }
        }
    }

    quote! {
        #[automatically_derived]
        impl Model for #ident {
            fn table_name() -> &'static str {
                #table_name
            }

            #id_fn
            #set_id_fn
            #ts_impl

            fn columns(&self) -> Vec<(&'static str, SqlValue)> {
                vec![
                    #(#column_entries,)*
                ]
            }

            fn from_row(row: &Row) -> Option<Self> {
                Some(Self {
                    #(#literals,)*
                })
            }
        }
    }
}

/// Entry point for `#[derive(Model)]`.
#[proc_macro_derive(Model, attributes(model))]
pub fn derive_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match expand(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand(input: &DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let config = ModelConfig::parse(&input.attrs)?;
    let fields = collect_fields(input, &config.primary_key)?;

    let model_impl = build_model_impl(input, &config, &fields);

    Ok(quote! {
        const _: () = {
            use torm::orm::model::Timestamps;
            use torm::db::db_types::{Row, SqlValue};
            use torm::chrono::{DateTime, Utc};
            use torm::Uuid;

            #model_impl
        };
    })
}
