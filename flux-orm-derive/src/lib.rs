extern crate proc_macro;

use darling::ast::Fields;
use darling::{FromDeriveInput, FromField, ast};
use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, Ident, Meta, Type, parse_macro_input};
use syn::{
    parse::{Parse, ParseStream},
    Expr, ExprField, Token,
};

// ============================================================================
// ATTRIBUTE PARSING
// ============================================================================

#[derive(Debug, FromDeriveInput)]
#[darling(attributes(flux), supports(struct_named))]
struct ModelOpts {
    ident: Ident,
    data: ast::Data<(), FieldOpts>,

    #[darling(default)]
    table: Option<String>,

    #[darling(default)]
    primary_key: Option<String>,

    #[darling(default)]
    timestamps: bool,

    #[darling(default)]
    soft_delete: bool,
}

#[derive(Debug, FromField)]
#[darling(attributes(flux))]
struct FieldOpts {
    ident: Option<Ident>,
    ty: Type,

    #[darling(default)]
    primary_key: bool,

    #[darling(default)]
    skip: bool,

    #[darling(default)]
    column: Option<String>,

    #[darling(default)]
    default: Option<String>,

    #[darling(default)]
    unique: bool,

    #[darling(default)]
    indexed: bool,
}

// ============================================================================
// DERIVE MACRO - #[derive(Model)]
// ============================================================================

#[proc_macro_derive(Model, attributes(flux))]
pub fn derive_model(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let opts = match ModelOpts::from_derive_input(&input) {
        Ok(opts) => opts,
        Err(e) => return TokenStream::from(e.write_errors()),
    };

    let expanded = impl_model(&opts);
    TokenStream::from(expanded)
}

fn generate_field_types(
    struct_name: &Ident,
    fields: Fields<&FieldOpts>,
) -> proc_macro2::TokenStream {
    let field_structs = fields.iter().filter(|f| !f.skip).map(|field| {
        let field_name = field.ident.as_ref().unwrap();
        let field_type = &field.ty;
        let column_name = field
            .column
            .as_ref()
            .map(|s| s.clone())
            .unwrap_or_else(|| to_snake_case(&field_name.to_string()));

        // Create globally unique field struct name
        let field_struct_name = Ident::new(
            &format!("{}_{}_Field", struct_name, to_pascal_case(&field_name.to_string())),
            field_name.span(),
        );

        // Extract inner type if Option
        let inner_type = if is_option_type(field_type) {
            extract_option_type(field_type).unwrap()
        } else {
            field_type
        };

        quote! {
            #[derive(Clone, Copy)]
            pub struct #field_struct_name;

            impl flux_orm::Field<#struct_name> for #field_struct_name {
                fn name(&self) -> &'static str {
                    #column_name
                }
                type Type = #inner_type;
            }

            impl flux_orm::Comparable<#struct_name> for #field_struct_name {}
            impl flux_orm::Orderable<#struct_name> for #field_struct_name {}

            // Convenience methods for building queries
            impl #field_struct_name {
                /// Create an equality condition
                pub fn eq<V: Into<flux_orm::backend::common_models::Value>>(self, value: V) -> flux_orm::FieldCondition<#struct_name> {
                    flux_orm::FieldCondition {
                        field_name: #column_name,
                        operator: flux_orm::ConditionOperator::Eq,
                        value: value.into(),
                        _marker: std::marker::PhantomData,
                    }
                }

                /// Create a not-equal condition
                pub fn ne<V: Into<flux_orm::backend::common_models::Value>>(self, value: V) -> flux_orm::FieldCondition<#struct_name> {
                    flux_orm::FieldCondition {
                        field_name: #column_name,
                        operator: flux_orm::ConditionOperator::Ne,
                        value: value.into(),
                        _marker: std::marker::PhantomData,
                    }
                }

                /// Create a greater-than condition
                pub fn gt<V: Into<flux_orm::backend::common_models::Value>>(self, value: V) -> flux_orm::FieldCondition<#struct_name> {
                    flux_orm::FieldCondition {
                        field_name: #column_name,
                        operator: flux_orm::ConditionOperator::Gt,
                        value: value.into(),
                        _marker: std::marker::PhantomData,
                    }
                }

                /// Create a greater-than-or-equal condition
                pub fn gte<V: Into<flux_orm::backend::common_models::Value>>(self, value: V) -> flux_orm::FieldCondition<#struct_name> {
                    flux_orm::FieldCondition {
                        field_name: #column_name,
                        operator: flux_orm::ConditionOperator::Gte,
                        value: value.into(),
                        _marker: std::marker::PhantomData,
                    }
                }

                /// Create a less-than condition
                pub fn lt<V: Into<flux_orm::backend::common_models::Value>>(self, value: V) -> flux_orm::FieldCondition<#struct_name> {
                    flux_orm::FieldCondition {
                        field_name: #column_name,
                        operator: flux_orm::ConditionOperator::Lt,
                        value: value.into(),
                        _marker: std::marker::PhantomData,
                    }
                }

                /// Create a less-than-or-equal condition
                pub fn lte<V: Into<flux_orm::backend::common_models::Value>>(self, value: V) -> flux_orm::FieldCondition<#struct_name> {
                    flux_orm::FieldCondition {
                        field_name: #column_name,
                        operator: flux_orm::ConditionOperator::Lte,
                        value: value.into(),
                        _marker: std::marker::PhantomData,
                    }
                }

                /// Create a LIKE pattern condition
                pub fn like<V: Into<flux_orm::backend::common_models::Value>>(self, pattern: V) -> flux_orm::FieldCondition<#struct_name> {
                    flux_orm::FieldCondition {
                        field_name: #column_name,
                        operator: flux_orm::ConditionOperator::Like,
                        value: pattern.into(),
                        _marker: std::marker::PhantomData,
                    }
                }

                /// Create an IN condition
                pub fn in_values<V: Into<flux_orm::backend::common_models::Value>>(self, values: Vec<V>) -> flux_orm::FieldCondition<#struct_name> {
                    let converted: Vec<flux_orm::backend::common_models::Value> = values.into_iter().map(|v| v.into()).collect();
                    flux_orm::FieldCondition {
                        field_name: #column_name,
                        operator: flux_orm::ConditionOperator::In(converted.clone()),
                        value: flux_orm::backend::common_models::Value::Null, // Заглушка
                        _marker: std::marker::PhantomData,
                    }
                }

                /// Create an IS NULL condition
                pub fn is_null(self) -> flux_orm::FieldCondition<#struct_name> {
                    flux_orm::FieldCondition {
                        field_name: #column_name,
                        operator: flux_orm::ConditionOperator::IsNull,
                        value: flux_orm::backend::common_models::Value::Null,
                        _marker: std::marker::PhantomData,
                    }
                }

                /// Create an IS NOT NULL condition
                pub fn is_not_null(self) -> flux_orm::FieldCondition<#struct_name> {
                    flux_orm::FieldCondition {
                        field_name: #column_name,
                        operator: flux_orm::ConditionOperator::IsNotNull,
                        value: flux_orm::backend::common_models::Value::Null,
                        _marker: std::marker::PhantomData,
                    }
                }

                /// Create an ascending order
                pub fn asc(self) -> flux_orm::FieldOrder<#struct_name> {
                    flux_orm::FieldOrder {
                        field_name: #column_name,
                        descending: false,
                        _marker: std::marker::PhantomData,
                    }
                }

                /// Create a descending order
                pub fn desc(self) -> flux_orm::FieldOrder<#struct_name> {
                    flux_orm::FieldOrder {
                        field_name: #column_name,
                        descending: true,
                        _marker: std::marker::PhantomData,
                    }
                }
            }
        }
    });

    let field_constants = fields.iter().filter(|f| !f.skip).map(|field| {
        let field_name = field.ident.as_ref().unwrap();
        let field_struct_name = Ident::new(
            &format!("{}_{}_Field", struct_name, to_pascal_case(&field_name.to_string())),
            field_name.span(),
        );
        let const_name = Ident::new(
            &to_screaming_snake_case(&field_name.to_string()),
            field_name.span(),
        );

        let column_name = field
            .column
            .as_ref()
            .map(|s| s.clone())
            .unwrap_or_else(|| to_snake_case(&field_name.to_string()));

        quote! {
            #[doc = concat!("Field accessor for the `", #column_name, "` column")]
            pub const #const_name: #field_struct_name = #field_struct_name;
        }
    });

    // Use struct name to create unique module name
    let fields_mod_name = Ident::new(&format!("{}_fields", to_snake_case(&struct_name.to_string())), struct_name.span());

    quote! {
        /// Сгенерированные типы полей для типобезопасных запросов
        #[allow(non_snake_case)]
        pub mod #fields_mod_name {
            use super::*;
            // Нам нужен PhantomData для полей _marker
            use std::marker::PhantomData;

            #(#field_structs)*

            /// Field constants for convenient access
            #(#field_constants)*
        }
    }
}

fn impl_model(opts: &ModelOpts) -> proc_macro2::TokenStream {
    let struct_name = &opts.ident;

    // Extract fields
    let fields = match &opts.data {
        ast::Data::Struct(fields) => fields,
        _ => panic!("Model can only be derived for structs"),
    };

    // Determine table name
    let table_name = opts
        .table
        .as_ref()
        .map(|s| s.clone())
        .unwrap_or_else(|| to_snake_case(&struct_name.to_string()) + "s");

    // Find primary key field
    let pk_field = fields
        .iter()
        .find(|f| f.primary_key)
        .or_else(|| {
            fields
                .iter()
                .find(|f| f.ident.as_ref().map(|i| i == "id").unwrap_or(false))
        })
        .expect("Model must have a primary key field");

    let pk_field_name = pk_field.ident.as_ref().unwrap();

    // Fix: Handle both Option and non-Option primary keys
    let pk_type = if is_option_type(&pk_field.ty) {
        extract_option_type(&pk_field.ty)
            .expect("Failed to extract Option<T> inner type for primary key")
    } else {
        &pk_field.ty
    };

    let pk_column_name = opts
        .primary_key
        .as_ref()
        .map(|s| s.clone())
        .unwrap_or_else(|| "id".to_string());

    // Generate implementations
    let model_impl = generate_model_impl(
        struct_name,
        &table_name,
        &pk_column_name,
        pk_field_name,
        pk_type,
        opts.timestamps,
    );

    let to_values_impl = generate_to_values(struct_name, fields.as_ref());
    let from_values_impl = generate_from_values(struct_name, fields.as_ref());

    let field_types = generate_field_types(struct_name, fields.as_ref());

    let mut output = quote! {
        #model_impl
        #to_values_impl
        #from_values_impl
        #field_types  // This was missing!
    };

    // Add Timestamps trait if enabled
    if opts.timestamps {
        let timestamps_impl = generate_timestamps_impl(struct_name, fields.as_ref());
        output = quote! {
            #output
            #timestamps_impl
        };
    }

    // Add SoftDelete trait if enabled
    if opts.soft_delete {
        let soft_delete_impl = generate_soft_delete_impl(struct_name, fields.as_ref());
        output = quote! {
            #output
            #soft_delete_impl
        };
    }

    output
}

fn generate_model_impl(
    struct_name: &Ident,
    table_name: &str,
    pk_column_name: &str,
    pk_field_name: &Ident,
    pk_type: &Type,
    has_timestamps: bool,
) -> proc_macro2::TokenStream {
    // Generate lifecycle hooks only if timestamps are enabled
    let lifecycle_hooks = if has_timestamps {
        quote! {
            async fn before_create(&mut self, _db: &flux_orm::Flux) -> flux_orm::backend::errors::Result<()> {
                let now = chrono::Utc::now();
                self.created_at = now;
                self.updated_at = now;
                Ok(())
            }

            async fn before_update(&mut self, _db: &flux_orm::Flux) -> flux_orm::backend::errors::Result<()> {
                self.updated_at = chrono::Utc::now();
                Ok(())
            }
        }
    } else {
        quote! {}
    };

    quote! {
        #[async_trait::async_trait]
        impl flux_orm::Model for #struct_name {
            const TABLE: &'static str = #table_name;
            const PRIMARY_KEY: &'static str = #pk_column_name;
            type Id = #pk_type;

            fn id(&self) -> Option<Self::Id> {
                self.#pk_field_name.clone()
            }

            fn set_id(&mut self, id: Self::Id) {
                self.#pk_field_name = Some(id);
            }

            fn to_values(&self) -> std::collections::HashMap<String, flux_orm::backend::common_models::Value> {
                Self::_to_values_impl(self)
            }

            fn from_values(values: std::collections::HashMap<String, flux_orm::backend::common_models::Value>) -> flux_orm::backend::errors::Result<Self> {
                Self::_from_values_impl(values)
            }

            #lifecycle_hooks
        }
    }
}

fn generate_to_values(struct_name: &Ident, fields: Fields<&FieldOpts>) -> proc_macro2::TokenStream {
    let field_conversions = fields.iter().filter(|f| !f.skip).map(|field| {
        let field_name = field.ident.as_ref().unwrap();
        let column_name = field
            .column
            .as_ref()
            .map(|s| s.clone())
            .unwrap_or_else(|| to_snake_case(&field_name.to_string()));

        let ty = &field.ty;

        // Check if field is Option<T>
        if is_option_type(ty) {
            quote! {
                if let Some(ref val) = self.#field_name {
                    map.insert(#column_name.to_string(), val.clone().into());
                }
            }
        } else {
            quote! {
                map.insert(#column_name.to_string(), self.#field_name.clone().into());
            }
        }
    });

    quote! {
        impl #struct_name {
            fn _to_values_impl(&self) -> std::collections::HashMap<String, flux_orm::backend::common_models::Value> {
                let mut map = std::collections::HashMap::new();
                #(#field_conversions)*
                map
            }
        }
    }
}

fn generate_from_values(
    struct_name: &Ident,
    fields: Fields<&FieldOpts>,
) -> proc_macro2::TokenStream {
    let field_assignments = fields.iter().map(|field| {
        let field_name = field.ident.as_ref().unwrap();
        let column_name = field
            .column
            .as_ref()
            .map(|s| s.clone())
            .unwrap_or_else(|| to_snake_case(&field_name.to_string()));
        let ty = &field.ty;

        if field.skip {
            // For fields with `skip` use default value
            if is_option_type(ty) {
                quote! { #field_name: None }
            } else if let Some(default) = &field.default {
                let default_expr = syn::parse_str::<syn::Expr>(default)
                    .unwrap_or_else(|_| panic!("Invalid default expression for field {}", field_name));
                quote! { #field_name: #default_expr }
            } else {
                quote! { #field_name: Default::default() }
            }
        } else {
            // Normal logic for non-skip fields
            if is_option_type(ty) {
                let inner_type = extract_option_type(ty).unwrap();
                let extract_method = get_extract_method(inner_type);
                quote! {
                    #field_name: values.get(#column_name).and_then(|v| #extract_method(v))
                }
            } else {
                let extract_method = get_extract_method(ty);
                if let Some(default) = &field.default {
                    let default_expr = syn::parse_str::<syn::Expr>(default)
                        .unwrap_or_else(|_| panic!("Invalid default expression for field {}", field_name));
                    quote! {
                        #field_name: values.get(#column_name)
                            .and_then(|v| #extract_method(v))
                            .unwrap_or_else(|| #default_expr)
                    }
                } else {
                    quote! {
                        #field_name: values.get(#column_name)
                            .and_then(|v| #extract_method(v))
                            .ok_or_else(|| flux_orm::backend::errors::FluxError::Serialization(
                                format!("Missing required field: {}", #column_name)
                            ))?
                    }
                }
            }
        }
    });

    quote! {
        impl #struct_name {
            fn _from_values_impl(values: std::collections::HashMap<String, flux_orm::backend::common_models::Value>) -> flux_orm::backend::errors::Result<Self> {
                Ok(Self {
                    #(#field_assignments),*
                })
            }
        }
    }
}

fn generate_timestamps_impl(
    struct_name: &Ident,
    fields: Fields<&FieldOpts>,
) -> proc_macro2::TokenStream {
    let _created_at_field = fields
        .iter()
        .find(|f| f.ident.as_ref().map(|i| i == "created_at").unwrap_or(false))
        .expect("Timestamps requires created_at field");

    let _updated_at_field = fields
        .iter()
        .find(|f| f.ident.as_ref().map(|i| i == "updated_at").unwrap_or(false))
        .expect("Timestamps requires updated_at field");

    quote! {
        impl flux_orm::Timestamps for #struct_name {
            fn created_at(&self) -> chrono::DateTime<chrono::Utc> {
                self.created_at
            }

            fn updated_at(&self) -> chrono::DateTime<chrono::Utc> {
                self.updated_at
            }

            fn set_created_at(&mut self, time: chrono::DateTime<chrono::Utc>) {
                self.created_at = time;
            }

            fn set_updated_at(&mut self, time: chrono::DateTime<chrono::Utc>) {
                self.updated_at = time;
            }
        }
    }
}

fn generate_soft_delete_impl(
    struct_name: &Ident,
    fields: Fields<&FieldOpts>,
) -> proc_macro2::TokenStream {
    let _deleted_at_field = fields
        .iter()
        .find(|f| f.ident.as_ref().map(|i| i == "deleted_at").unwrap_or(false))
        .expect("SoftDelete requires deleted_at field");

    quote! {
        #[async_trait::async_trait]
        impl flux_orm::SoftDelete for #struct_name {
            fn deleted_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
                self.deleted_at
            }

            fn set_deleted_at(&mut self, time: Option<chrono::DateTime<chrono::Utc>>) {
                self.deleted_at = time;
            }
        }
    }
}

// ============================================================================
// RELATION MACROS
// ============================================================================

#[proc_macro_attribute]
pub fn has_many(args: TokenStream, input: TokenStream) -> TokenStream {
    let args_parsed = parse_macro_input!(args with syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated);
    let input = parse_macro_input!(input as syn::ItemImpl);

    // Parse arguments: target model and foreign key
    let target_model = extract_meta_arg(&args_parsed, 0);
    let foreign_key = extract_meta_arg(&args_parsed, 1);

    let self_type = &input.self_ty;

    let expanded = quote! {
        #input

        #[async_trait::async_trait]
        impl flux_orm::HasMany<#target_model> for #self_type {
            fn foreign_key() -> &'static str {
                #foreign_key
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_attribute]
pub fn has_one(args: TokenStream, input: TokenStream) -> TokenStream {
    let args_parsed = parse_macro_input!(args with syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated);
    let input = parse_macro_input!(input as syn::ItemImpl);

    let target_model = extract_meta_arg(&args_parsed, 0);
    let foreign_key = extract_meta_arg(&args_parsed, 1);

    let self_type = &input.self_ty;

    let expanded = quote! {
        #input

        #[async_trait::async_trait]
        impl flux_orm::HasOne<#target_model> for #self_type {
            fn foreign_key() -> &'static str {
                #foreign_key
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_attribute]
pub fn belongs_to(args: TokenStream, input: TokenStream) -> TokenStream {
    let args_parsed = parse_macro_input!(args with syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated);
    let input = parse_macro_input!(input as syn::ItemImpl);

    let target_model = extract_meta_arg(&args_parsed, 0);
    let foreign_key_field = extract_meta_arg(&args_parsed, 1);

    let self_type = &input.self_ty;

    let expanded = quote! {
        #input

        #[async_trait::async_trait]
        impl flux_orm::BelongsTo<#target_model> for #self_type {
            fn foreign_key_value(&self) -> Option<<#target_model as flux_orm::Model>::Id> {
                self.#foreign_key_field.clone()
            }
        }
    };

    TokenStream::from(expanded)
}

#[proc_macro_attribute]
pub fn belongs_to_many(args: TokenStream, input: TokenStream) -> TokenStream {
    let args_parsed = parse_macro_input!(args with syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated);
    let input = parse_macro_input!(input as syn::ItemImpl);

    let target_model = extract_meta_arg(&args_parsed, 0);
    let pivot_table = extract_meta_arg(&args_parsed, 1);
    let foreign_key = extract_meta_arg(&args_parsed, 2);
    let related_key = extract_meta_arg(&args_parsed, 3);

    let self_type = &input.self_ty;

    let expanded = quote! {
        #input

        #[async_trait::async_trait]
        impl flux_orm::BelongsToMany<#target_model> for #self_type {
            fn pivot_table() -> &'static str {
                #pivot_table
            }

            fn foreign_key() -> &'static str {
                #foreign_key
            }

            fn related_key() -> &'static str {
                #related_key
            }
        }
    };

    TokenStream::from(expanded)
}

// ============================================================================
// MIGRATION BUILDER MACRO
// ============================================================================

#[proc_macro]
pub fn migration(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::ExprArray);

    let migrations = input.elems.iter().enumerate().map(|(idx, elem)| {
        if let syn::Expr::Struct(expr_struct) = elem {
            let version = idx + 1;

            let name = find_field_value(&expr_struct.fields, "name");
            let up = find_field_value(&expr_struct.fields, "up");
            let down = find_field_value(&expr_struct.fields, "down");

            quote! {
                flux_orm::Migration::new(
                    #version as i64,
                    #name,
                    #up,
                    #down
                )
            }
        } else {
            panic!("Expected struct expression in migration array");
        }
    });

    let expanded = quote! {
        vec![
            #(#migrations),*
        ]
    };

    TokenStream::from(expanded)
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch.is_uppercase() {
            if !result.is_empty() {
                result.push('_');
            }
            result.push(ch.to_lowercase().next().unwrap());
        } else {
            result.push(ch);
        }
    }

    result
}

fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Option";
        }
    }
    false
}

fn extract_option_type(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        return Some(inner_ty);
                    }
                }
            }
        }
    }
    None
}

fn get_extract_method(ty: &Type) -> proc_macro2::TokenStream {
    let type_str = quote!(#ty).to_string().replace(" ", "");

    match type_str.as_str() {
        "i16" => quote! { flux_orm::backend::common_models::Value::as_i16 },
        "i32" => quote! { flux_orm::backend::common_models::Value::as_i32 },
        "i64" => quote! { flux_orm::backend::common_models::Value::as_i64 },
        "f32" => quote! { flux_orm::backend::common_models::Value::as_f32 },
        "f64" => quote! { flux_orm::backend::common_models::Value::as_f64 },
        "bool" => quote! { flux_orm::backend::common_models::Value::as_bool },
        "String" => quote! { flux_orm::backend::common_models::Value::as_string },
        s if s.contains("DateTime") => quote! { flux_orm::backend::common_models::Value::as_datetime },
        s if s.contains("Uuid") => quote! { flux_orm::backend::common_models::Value::as_uuid },
        s if s.contains("Value") => quote! { flux_orm::backend::common_models::Value::as_json },
        _ => quote! { |v| Some(v.clone()) },
    }
}

fn extract_meta_arg(
    args: &syn::punctuated::Punctuated<Meta, syn::Token![,]>,
    index: usize,
) -> proc_macro2::TokenStream {
    if let Some(meta) = args.iter().nth(index) {
        match meta {
            Meta::Path(path) => quote! { #path },
            Meta::NameValue(nv) => {
                let value = &nv.value;
                quote! { #value }
            }
            Meta::List(_) => panic!("List meta not supported at index {}", index),
        }
    } else {
        panic!("Missing argument at index {}", index);
    }
}

fn find_field_value(
    fields: &syn::punctuated::Punctuated<syn::FieldValue, syn::token::Comma>,
    name: &str,
) -> proc_macro2::TokenStream {
    for field in fields {
        if let syn::Member::Named(ident) = &field.member {
            if ident == name {
                let expr = &field.expr;
                return quote! { #expr };
            }
        }
    }
    panic!("Field '{}' not found in migration struct", name);
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

fn to_screaming_snake_case(s: &str) -> String {
    to_snake_case(s).to_uppercase()
}

// ============================================================================
// PART 4: Type-Safe Query Macro
// ============================================================================

struct TypeSafeQuery {
    model: Ident,
    operations: Vec<TypeSafeOperation>,
}

enum TypeSafeOperation {
    Where {
        field: ExprField,
        op: CompareOp,
        value: Expr,
    },
    OrderBy {
        field: ExprField,
        desc: bool,
    },
    Select {
        fields: Vec<ExprField>,
    },
    Limit(usize),
    Offset(usize),
}

enum CompareOp {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    Like,
    In,
    IsNull,
    IsNotNull,
}

impl Parse for TypeSafeQuery {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let model: Ident = input.parse()?;
        let mut operations = Vec::new();

        while !input.is_empty() {
            if !input.peek(Token![.]) {
                break;
            }
            input.parse::<Token![.]>()?;

            let method: Ident = input.parse()?;
            let content;
            syn::parenthesized!(content in input);

            match method.to_string().as_str() {
                "where_" => {
                    let field: ExprField = content.parse()?;
                    let op = parse_compare_op(&content)?;
                    let value: Expr = content.parse()?;
                    operations.push(TypeSafeOperation::Where { field, op, value });
                }
                "order_by" => {
                    let field: ExprField = content.parse()?;
                    let desc = if content.peek(Token![,]) {
                        content.parse::<Token![,]>()?;
                        let dir: Ident = content.parse()?;
                        dir.to_string() == "desc"
                    } else {
                        false
                    };
                    operations.push(TypeSafeOperation::OrderBy { field, desc });
                }
                "select" => {
                    let fields = content
                        .parse_terminated(ExprField::parse, Token![,])?
                        .into_iter()
                        .collect();
                    operations.push(TypeSafeOperation::Select { fields });
                }
                "limit" => {
                    let n: syn::LitInt = content.parse()?;
                    operations.push(TypeSafeOperation::Limit(n.base10_parse()?));
                }
                "offset" => {
                    let n: syn::LitInt = content.parse()?;
                    operations.push(TypeSafeOperation::Offset(n.base10_parse()?));
                }
                _ => return Err(syn::Error::new(method.span(), "Unknown query method")),
            }
        }

        Ok(TypeSafeQuery { model, operations })
    }
}

fn parse_compare_op(input: ParseStream) -> syn::Result<CompareOp> {
    let lookahead = input.lookahead1();

    if lookahead.peek(Token![==]) {
        input.parse::<Token![==]>()?;
        Ok(CompareOp::Eq)
    } else if lookahead.peek(Token![!=]) {
        input.parse::<Token![!=]>()?;
        Ok(CompareOp::Ne)
    } else if lookahead.peek(Token![>]) {
        input.parse::<Token![>]>()?;
        if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            Ok(CompareOp::Gte)
        } else {
            Ok(CompareOp::Gt)
        }
    } else if lookahead.peek(Token![<]) {
        input.parse::<Token![<]>()?;
        if input.peek(Token![=]) {
            input.parse::<Token![=]>()?;
            Ok(CompareOp::Lte)
        } else {
            Ok(CompareOp::Lt)
        }
    } else {
        Err(lookahead.error())
    }
}

// ============================================================================
// QUERY BUILDER MACRO
// ============================================================================

#[proc_macro]
pub fn query(input: TokenStream) -> TokenStream {
    let query = parse_macro_input!(input as TypeSafeQuery);
    let model = &query.model;

    let mut builder = quote! {
        flux_orm::Query::<#model>::new()
    };

    for op in query.operations {
        match op {
            TypeSafeOperation::Where { field, op, value } => {
                let method = match op {
                    CompareOp::Eq => quote! { where_field },
                    CompareOp::Ne => quote! { where_field_ne },
                    CompareOp::Gt => quote! { where_field_gt },
                    CompareOp::Gte => quote! { where_field_gte },
                    CompareOp::Lt => quote! { where_field_lt },
                    CompareOp::Lte => quote! { where_field_lte },
                    CompareOp::Like => quote! { where_field_like },
                    CompareOp::In => quote! { where_field_in },
                    CompareOp::IsNull => quote! { where_field_null },
                    CompareOp::IsNotNull => quote! { where_field_not_null },
                };

                if matches!(op, CompareOp::IsNull | CompareOp::IsNotNull) {
                    builder = quote! { #builder.#method(#field) };
                } else {
                    builder = quote! { #builder.#method(#field, #value) };
                }
            }
            TypeSafeOperation::OrderBy { field, desc } => {
                let method = if desc {
                    quote! { order_by_field_desc }
                } else {
                    quote! { order_by_field }
                };
                builder = quote! { #builder.#method(#field) };
            }
            TypeSafeOperation::Select { fields } => {
                builder = quote! { #builder.select_fields(&[#(#fields),*]) };
            }
            TypeSafeOperation::Limit(n) => {
                builder = quote! { #builder.limit(#n) };
            }
            TypeSafeOperation::Offset(n) => {
                builder = quote! { #builder.offset(#n) };
            }
        }
    }

    TokenStream::from(builder)
}

// ============================================================================
// VALIDATION MACRO
// ============================================================================

#[proc_macro_derive(Validate, attributes(validate))]
pub fn derive_validate(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let struct_name = &input.ident;

    let expanded = quote! {
        impl #struct_name {
            pub fn validate(&self) -> flux_orm::backend::errors::Result<()> {
                let mut errors = Vec::new();

                // Validation logic would be generated here based on attributes

                if errors.is_empty() {
                    Ok(())
                } else {
                    Err(flux_orm::backend::errors::FluxError::Validation(errors))
                }
            }
        }
    };

    TokenStream::from(expanded)
}