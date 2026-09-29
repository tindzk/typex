//! Derive macros for the [`typex`](https://docs.rs/typex) reflection traits.
//!
//! [`Meta`] supports structs and enums. Add `#[typex(opaque)]` to expose only
//! runtime type information, or `#[typex(partial_eq)]` to delegate equality to
//! the type's `PartialEq` implementation. [`MetaMut`] adds structural mutation
//! and requires [`Meta`] to be derived as well.
//!
//! Generated code refers to `::typex`, `::core` and fully qualified prelude
//! items, so local items named `Result`, `Option` or `core` do not affect it.
//! Generated bindings use a `__typex_` prefix to avoid capturing field names.
//!
//! For generic types, the derives add a [`Meta`] or [`MetaMut`] bound for each
//! field type that mentions a type parameter. In a field type that contains
//! the derived type, they bound each part that mentions a type parameter but
//! not the derived type, instead of the whole field. This keeps types such as
//! `struct Tree<T> { value: T, children: Vec<Tree<T>> }` from overflowing
//! trait resolution.

#![forbid(unsafe_code)]

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2, TokenTree};
use quote::{ToTokens, format_ident, quote};
use syn::ext::IdentExt;
use syn::visit::Visit;
use syn::{
  Attribute, Data, DataEnum, DataStruct, DeriveInput, Fields, GenericParam, Generics, Ident, Index,
  Member, Type, Variant, parse_macro_input, parse_quote,
};

/// Derives structural [`typex::Meta`](https://docs.rs/typex/latest/typex/trait.Meta.html)
/// for a struct or enum.
///
/// By default, fields, tuple items and active enum variants are exposed.
/// Use `#[typex(opaque)]` to hide the structure. Use
/// `#[typex(partial_eq)]` to delegate dynamic equality to `PartialEq`.
#[proc_macro_derive(Meta, attributes(typex))]
pub fn meta_derive(input: TokenStream) -> TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  expand_meta(&input)
    .unwrap_or_else(syn::Error::into_compile_error)
    .into()
}

/// Derives mutable
/// [`typex::MetaMut`](https://docs.rs/typex/latest/typex/trait.MetaMut.html) for
/// a struct or enum.
///
/// Derive [`Meta`] separately on the same type. The `opaque` option is shared
/// with [`Meta`]; mutable structural access is generated only for exposed
/// fields and items.
#[proc_macro_derive(MetaMut, attributes(typex))]
pub fn meta_mut_derive(input: TokenStream) -> TokenStream {
  let input = parse_macro_input!(input as DeriveInput);
  expand_meta_mut(&input)
    .unwrap_or_else(syn::Error::into_compile_error)
    .into()
}

fn expand_meta(input: &DeriveInput) -> syn::Result<TokenStream2> {
  let options = typex_options(&input.attrs)?;
  reject_union(input, "Meta")?;

  let partial_eq_fn = options.partial_eq.then(|| {
    quote! {
      fn eq_dyn(&self, __typex_other: &dyn ::typex::Meta) -> ::core::primitive::bool {
        match ::typex::Meta::as_any(__typex_other).downcast_ref::<Self>() {
          ::core::option::Option::Some(__typex_value) => self == __typex_value,
          ::core::option::Option::None => false,
        }
      }
    }
  });

  let (shape, access, path_constants, mut generics) = if options.opaque {
    let shape = quote! { ::typex::Reflect::Scalar };
    (shape, None, quote! {}, input.generics.clone())
  } else {
    let access = match &input.data {
      Data::Struct(data) => struct_meta(data),
      Data::Enum(data) => enum_meta(data),
      Data::Union(_) => unreachable!("unions are rejected above"),
    };
    let eq = match &input.data {
      Data::Struct(data) => struct_eq(data),
      Data::Enum(data) => enum_eq(data),
      Data::Union(_) => unreachable!("unions are rejected above"),
    };
    let access = (access, eq);
    let constants = match &input.data {
      Data::Struct(data) => struct_path_constants(input, data),
      Data::Enum(data) => enum_path_constants(input, data),
      Data::Union(_) => unreachable!("unions are rejected above"),
    };
    let generics = bounded_generics(input, quote!(::typex::Meta));
    let shape = quote! { ::typex::Reflect::Struct(self) };
    (shape, Some(access), constants, generics)
  };

  let where_clause = generics.make_where_clause();
  if options.partial_eq {
    where_clause
      .predicates
      .push(parse_quote!(Self: ::core::cmp::PartialEq));
  }
  where_clause.predicates.push(parse_quote!(Self: 'static));

  let name = &input.ident;
  let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
  // `partial_eq` takes precedence over the generated structural comparison.
  let (access, structural_eq_fn) = access.unzip();
  let eq_fn = partial_eq_fn.or(structural_eq_fn);
  let access_impl = access.map(|access| {
    quote! {
      impl #impl_generics ::typex::StructAccess for #name #ty_generics #where_clause {
        #access
      }
    }
  });

  Ok(quote! {
    impl #impl_generics ::typex::Meta for #name #ty_generics #where_clause {
      fn reflect(&self) -> ::typex::Reflect<'_> {
        #shape
      }

      #eq_fn

      fn into_any(
        self: ::typex::__private::Box<Self>,
      ) -> ::typex::__private::Box<dyn ::typex::__private::Any> {
        self
      }

      fn as_any(&self) -> &dyn ::typex::__private::Any {
        self
      }
    }

    #access_impl

    #path_constants
  })
}

fn expand_meta_mut(input: &DeriveInput) -> syn::Result<TokenStream2> {
  let options = typex_options(&input.attrs)?;
  reject_union(input, "MetaMut")?;

  let (shape, access, mut generics) = if options.opaque {
    let shape = quote! { ::typex::ReflectMut::Opaque };
    (shape, None, input.generics.clone())
  } else {
    let access = match &input.data {
      Data::Struct(data) => struct_meta_mut(data),
      Data::Enum(data) => enum_meta_mut(data),
      Data::Union(_) => unreachable!("unions are rejected above"),
    };
    let shape = quote! { ::typex::ReflectMut::Struct(self) };
    (
      shape,
      Some(access),
      bounded_generics(input, quote!(::typex::MetaMut)),
    )
  };
  generics
    .make_where_clause()
    .predicates
    .push(parse_quote!(Self: ::typex::Meta));

  let name = &input.ident;
  let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
  let access_impl = access.map(|access| {
    quote! {
      impl #impl_generics ::typex::StructAccessMut for #name #ty_generics #where_clause {
        #access
      }
    }
  });

  Ok(quote! {
    impl #impl_generics ::typex::MetaMut for #name #ty_generics #where_clause {
      fn reflect_mut(&mut self) -> ::typex::ReflectMut<'_> {
        #shape
      }

      fn replace_dyn(
        &mut self,
        __typex_value: ::typex::Object,
      ) -> ::core::result::Result<::typex::Object, ::typex::Object> {
        if !::typex::Meta::as_any(&*__typex_value).is::<Self>() {
          return ::core::result::Result::Err(__typex_value);
        }

        let __typex_replacement = *::typex::Meta::into_any(__typex_value.into_inner())
          .downcast::<Self>()
          .unwrap();
        ::core::result::Result::Ok(::typex::Object::new(::core::mem::replace(
          self,
          __typex_replacement,
        )))
      }

      fn set_dyn(
        &mut self,
        __typex_value: ::typex::Object,
      ) -> ::core::result::Result<(), ::typex::Object> {
        if !::typex::Meta::as_any(&*__typex_value).is::<Self>() {
          return ::core::result::Result::Err(__typex_value);
        }

        *self = *::typex::Meta::into_any(__typex_value.into_inner())
          .downcast::<Self>()
          .unwrap();
        ::core::result::Result::Ok(())
      }

      fn as_meta(&self) -> &dyn ::typex::Meta {
        self
      }

      fn as_any_mut(&mut self) -> &mut dyn ::typex::__private::Any {
        self
      }
    }

    #access_impl
  })
}

struct TypexOptions {
  opaque: bool,
  partial_eq: bool,
}

fn typex_options(attrs: &[Attribute]) -> syn::Result<TypexOptions> {
  let mut options = TypexOptions {
    opaque: false,
    partial_eq: false,
  };
  for attr in attrs.iter().filter(|attr| attr.path().is_ident("typex")) {
    attr.parse_nested_meta(|meta| {
      if meta.path.is_ident("opaque") {
        options.opaque = true;
      } else if meta.path.is_ident("partial_eq") {
        options.partial_eq = true;
      } else {
        return Err(meta.error("unsupported typex option; expected `opaque` or `partial_eq`"));
      }
      Ok(())
    })?;
  }
  Ok(options)
}

fn reject_union(input: &DeriveInput, derive: &str) -> syn::Result<()> {
  match input.data {
    Data::Union(_) => Err(syn::Error::new_spanned(
      input,
      format!("{derive} can only be derived for structs and enums"),
    )),
    _ => Ok(()),
  }
}

/// Adds `bound` for each type returned by [`bounded_field_types`], once per
/// distinct type.
fn bounded_generics(input: &DeriveInput, bound: TokenStream2) -> Generics {
  let mut generics = input.generics.clone();
  let params = input
    .generics
    .params
    .iter()
    .filter_map(|param| match param {
      GenericParam::Type(param) => Some(param.ident.clone()),
      _ => None,
    })
    .collect::<Vec<_>>();
  if params.is_empty() {
    return generics;
  }

  // `syn::Type` implements `PartialEq` only with the `extra-traits` feature, so
  // compare token strings instead.
  let mut seen = Vec::<String>::new();
  let where_clause = generics.make_where_clause();
  for ty in field_types(&input.data) {
    for bounded_ty in bounded_field_types(ty, &input.ident, &params) {
      let key = bounded_ty.to_token_stream().to_string();
      if !seen.contains(&key) {
        seen.push(key);
        where_clause
          .predicates
          .push(parse_quote!(#bounded_ty: #bound));
      }
    }
  }
  generics
}

fn field_types(data: &Data) -> Vec<&Type> {
  match data {
    Data::Struct(data) => data.fields.iter().map(|field| &field.ty).collect(),
    Data::Enum(data) => data
      .variants
      .iter()
      .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
      .collect(),
    Data::Union(_) => Vec::new(),
  }
}

fn mentions_any(tokens: &TokenStream2, idents: &[Ident]) -> bool {
  tokens.clone().into_iter().any(|token| match token {
    TokenTree::Ident(ident) => idents.contains(&ident),
    TokenTree::Group(group) => mentions_any(&group.stream(), idents),
    _ => false,
  })
}

fn mentions_derived_type(ty: &Type, name: &Ident) -> bool {
  let mut visitor = DerivedTypeVisitor { name, found: false };
  visitor.visit_type(ty);
  visitor.found
}

struct DerivedTypeVisitor<'a> {
  name: &'a Ident,
  found: bool,
}

impl<'ast> Visit<'ast> for DerivedTypeVisitor<'_> {
  fn visit_type_path(&mut self, type_path: &'ast syn::TypePath) {
    self.found |= is_derived_type_path(type_path, self.name);
    syn::visit::visit_type_path(self, type_path);
  }
}

fn is_derived_type_path(type_path: &syn::TypePath, name: &Ident) -> bool {
  type_path.qself.is_none()
    && type_path.path.segments.len() == 1
    && matches!(
      type_path.path.segments.first(),
      Some(segment) if segment.ident == *name || segment.ident == "Self"
    )
}

/// Returns the parts of `ty` that need a capability bound. A type that does not
/// contain the derived type is returned whole if it mentions a type parameter.
/// Otherwise, the result holds each part that mentions a type parameter but not
/// the derived type.
fn bounded_field_types(ty: &Type, name: &Ident, params: &[Ident]) -> Vec<Type> {
  let mut visitor = BoundedFieldTypeVisitor {
    name,
    params,
    found: Vec::new(),
  };
  visitor.visit_type(ty);
  visitor.found
}

struct BoundedFieldTypeVisitor<'a> {
  name: &'a Ident,
  params: &'a [Ident],
  found: Vec<Type>,
}

impl<'ast> Visit<'ast> for BoundedFieldTypeVisitor<'_> {
  fn visit_type(&mut self, ty: &'ast Type) {
    if mentions_derived_type(ty, self.name) {
      syn::visit::visit_type(self, ty);
    } else if mentions_any(&ty.to_token_stream(), self.params) {
      self.found.push(ty.clone());
    }
  }

  fn visit_type_path(&mut self, type_path: &'ast syn::TypePath) {
    if !is_derived_type_path(type_path, self.name) {
      syn::visit::visit_type_path(self, type_path);
    }
  }
}

/// Returns the reflective name of a field or variant without any `r#` prefix.
fn reflective_name(ident: &Ident) -> String {
  ident.unraw().to_string()
}

fn field_member(index: usize, ident: Option<&Ident>) -> Member {
  match ident {
    Some(ident) => Member::Named(ident.clone()),
    None => Member::Unnamed(Index::from(index)),
  }
}

/// Returns the reflective name of the field at `index`.
fn field_key(index: usize, ident: Option<&Ident>) -> String {
  match ident {
    Some(ident) => reflective_name(ident),
    None => index.to_string(),
  }
}

fn path_constant(parts: &[&str], span: Span) -> Ident {
  Ident::new(&format!("FIELD_{}", parts.join("_").to_uppercase()), span)
}

fn binding(index: usize) -> Ident {
  format_ident!("__typex_field_{}", index)
}

fn other_binding(index: usize) -> Ident {
  format_ident!("__typex_other_{}", index)
}

/// Returns a pattern that binds every field of `variant` to `__typex_field_N`.
fn variant_bindings(variant: &Variant) -> TokenStream2 {
  variant_pattern(variant, binding)
}

/// Returns a pattern that binds every field of `variant` to the name that
/// `binding` returns for its index.
fn variant_pattern(variant: &Variant, binding: fn(usize) -> Ident) -> TokenStream2 {
  let ident = &variant.ident;
  match &variant.fields {
    Fields::Named(fields) => {
      let bindings = fields.named.iter().enumerate().map(|(index, field)| {
        let member = field.ident.as_ref().unwrap();
        let binding = binding(index);
        quote! { #member: #binding }
      });
      quote! { Self::#ident { #(#bindings),* } }
    }
    Fields::Unnamed(fields) => {
      let bindings = (0..fields.unnamed.len()).map(binding);
      quote! { Self::#ident(#(#bindings),*) }
    }
    Fields::Unit => quote! { Self::#ident },
  }
}

/// Generates an `eq_dyn` that evaluates `body` with `__typex_other`
/// downcast to `Self`.
///
/// The result matches the default structural comparison, which compares the
/// same exposed fields by name, without dispatching through the access traits.
fn eq_dyn_fn(body: TokenStream2) -> TokenStream2 {
  quote! {
    fn eq_dyn(&self, __typex_other: &dyn ::typex::Meta) -> ::core::primitive::bool {
      match ::typex::Meta::as_any(__typex_other).downcast_ref::<Self>() {
        ::core::option::Option::Some(__typex_other) => #body,
        ::core::option::Option::None => false,
      }
    }
  }
}

/// Returns `true` followed by an `eq_dyn` comparison for each pair of fields.
fn fields_eq(pairs: impl Iterator<Item = (TokenStream2, TokenStream2)>) -> TokenStream2 {
  let comparisons = pairs.map(|(left, right)| {
    quote! { && ::typex::Meta::eq_dyn(#left, #right) }
  });
  quote! { true #(#comparisons)* }
}

fn struct_eq(data: &DataStruct) -> TokenStream2 {
  let body = fields_eq(data.fields.iter().enumerate().map(|(index, field)| {
    let member = field_member(index, field.ident.as_ref());
    (quote! { &self.#member }, quote! { &__typex_other.#member })
  }));
  eq_dyn_fn(body)
}

fn enum_eq(data: &DataEnum) -> TokenStream2 {
  let arms = data.variants.iter().map(|variant| {
    let pattern = variant_pattern(variant, binding);
    let other_pattern = variant_pattern(variant, other_binding);
    let body = fields_eq((0..variant.fields.len()).map(|index| {
      let (left, right) = (binding(index), other_binding(index));
      (quote! { #left }, quote! { #right })
    }));
    quote! { (#pattern, #other_pattern) => #body }
  });
  // A single variant always matches, so a fallback arm would be unreachable.
  let fallback = (data.variants.len() > 1).then(|| quote! { _ => false, });
  let body = if data.variants.is_empty() {
    quote! { match *self {} }
  } else {
    quote! {
      match (self, __typex_other) {
        #(#arms,)*
        #fallback
      }
    }
  };
  eq_dyn_fn(body)
}

fn variant_wildcard(variant: &Variant) -> TokenStream2 {
  let ident = &variant.ident;
  match &variant.fields {
    Fields::Named(_) => quote! { Self::#ident { .. } },
    Fields::Unnamed(_) => quote! { Self::#ident(..) },
    Fields::Unit => quote! { Self::#ident },
  }
}

fn struct_path_constants(input: &DeriveInput, data: &DataStruct) -> TokenStream2 {
  let Fields::Named(fields) = &data.fields else {
    return quote! {};
  };

  let constants = fields.named.iter().map(|field| {
    let ident = field.ident.as_ref().unwrap();
    let key = reflective_name(ident);
    let constant = path_constant(&[&key], ident.span());
    let ty = &field.ty;
    quote! {
      #[doc(hidden)]
      pub const #constant: ::typex::TypedField<Self, #ty> = ::typex::TypedField::new(#key);
    }
  });

  path_constants_impl(input, constants)
}

fn enum_path_constants(input: &DeriveInput, data: &DataEnum) -> TokenStream2 {
  let constants = data.variants.iter().flat_map(|variant| {
    let variant_name = reflective_name(&variant.ident);
    let variant_segment = quote! { ::typex::PathSegment::Field(#variant_name) };
    let variant_constant = path_constant(&[&variant_name], variant.ident.span());

    let field_constants = variant
      .fields
      .iter()
      .enumerate()
      .map(|(index, field)| {
        let key = field_key(index, field.ident.as_ref());
        let span = field
          .ident
          .as_ref()
          .map_or_else(|| variant.ident.span(), Ident::span);
        let constant = path_constant(&[&variant_name, &key], span);
        let segment = match field.ident {
          Some(_) => quote! { ::typex::PathSegment::Field(#key) },
          None => quote! { ::typex::PathSegment::Item(#index) },
        };
        let ty = &field.ty;
        quote! {
          #[doc(hidden)]
          pub const #constant: ::typex::TypedField<Self, #ty> =
            ::typex::TypedField::from_segments(&[#variant_segment, #segment]);
        }
      })
      .collect::<Vec<_>>();

    core::iter::once(quote! {
      #[doc(hidden)]
      pub const #variant_constant: ::typex::TypedField<Self, Self> =
        ::typex::TypedField::from_segments(&[#variant_segment]);
    })
    .chain(field_constants)
  });

  path_constants_impl(input, constants)
}

fn path_constants_impl(
  input: &DeriveInput,
  constants: impl Iterator<Item = TokenStream2>,
) -> TokenStream2 {
  let name = &input.ident;
  let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
  quote! {
    impl #impl_generics #name #ty_generics #where_clause {
      #(#constants)*
    }
  }
}

fn struct_meta(data: &DataStruct) -> TokenStream2 {
  let keys = data
    .fields
    .iter()
    .enumerate()
    .map(|(index, field)| field_key(index, field.ident.as_ref()))
    .collect::<Vec<_>>();
  let members = data
    .fields
    .iter()
    .enumerate()
    .map(|(index, field)| field_member(index, field.ident.as_ref()))
    .collect::<Vec<_>>();

  let (item_arms, len) = match &data.fields {
    Fields::Unnamed(fields) => {
      let arms = members.iter().enumerate().map(|(index, member)| {
        quote! {
          #index => ::core::option::Option::Some(
            ::typex::ObjectRef::new(&self.#member),
          )
        }
      });
      let len = fields.unnamed.len();
      (
        arms.collect::<Vec<_>>(),
        quote! { ::core::option::Option::Some(#len) },
      )
    }
    _ => (Vec::new(), quote! { ::core::option::Option::None }),
  };

  quote! {
    fn field(
      &self,
      __typex_name: &::core::primitive::str,
    ) -> ::core::option::Option<::typex::ObjectRef<'_>> {
      match __typex_name {
        #(#keys => ::core::option::Option::Some(
          ::typex::ObjectRef::new(&self.#members),
        ),)*
        _ => ::core::option::Option::None,
      }
    }

    fn field_names(&self) -> &'static [&'static ::core::primitive::str] {
      &[#(#keys),*]
    }

    fn item(
      &self,
      __typex_index: ::core::primitive::usize,
    ) -> ::core::option::Option<::typex::ObjectRef<'_>> {
      match __typex_index {
        #(#item_arms,)*
        _ => ::core::option::Option::None,
      }
    }

    fn len(&self) -> ::core::option::Option<::core::primitive::usize> {
      #len
    }
  }
}

fn struct_meta_mut(data: &DataStruct) -> TokenStream2 {
  let keys = data
    .fields
    .iter()
    .enumerate()
    .map(|(index, field)| field_key(index, field.ident.as_ref()))
    .collect::<Vec<_>>();
  let members = data
    .fields
    .iter()
    .enumerate()
    .map(|(index, field)| field_member(index, field.ident.as_ref()))
    .collect::<Vec<_>>();
  let item_arms = match &data.fields {
    Fields::Unnamed(_) => members
      .iter()
      .enumerate()
      .map(|(index, member)| {
        quote! {
          #index => ::core::option::Option::Some(
            ::typex::ObjectRefMut::new(&mut self.#member),
          )
        }
      })
      .collect::<Vec<_>>(),
    _ => Vec::new(),
  };

  quote! {
    fn field_mut(
      &mut self,
      __typex_name: &::core::primitive::str,
    ) -> ::core::option::Option<::typex::ObjectRefMut<'_>> {
      match __typex_name {
        #(#keys => ::core::option::Option::Some(
          ::typex::ObjectRefMut::new(&mut self.#members),
        ),)*
        _ => ::core::option::Option::None,
      }
    }

    fn item_mut(
      &mut self,
      __typex_index: ::core::primitive::usize,
    ) -> ::core::option::Option<::typex::ObjectRefMut<'_>> {
      match __typex_index {
        #(#item_arms,)*
        _ => ::core::option::Option::None,
      }
    }
  }
}

/// Returns `match self { ... }` over `arms`, or `match *self {}` for an enum
/// without variants.
fn match_self(data: &DataEnum, arms: Vec<TokenStream2>) -> TokenStream2 {
  if data.variants.is_empty() {
    quote! { match *self {} }
  } else {
    quote! { match self { #(#arms,)* } }
  }
}

fn enum_meta(data: &DataEnum) -> TokenStream2 {
  let field_arms = data
    .variants
    .iter()
    .map(|variant| {
      let pattern = variant_bindings(variant);
      let variant_name = reflective_name(&variant.ident);
      let keys = variant
        .fields
        .iter()
        .enumerate()
        .map(|(index, field)| field_key(index, field.ident.as_ref()));
      let bindings = (0..variant.fields.len()).map(binding);
      quote! {
        #pattern => match __typex_name {
          #variant_name => ::core::option::Option::Some(
            ::typex::ObjectRef::new(self),
          ),
          #(#keys => ::core::option::Option::Some(
            ::typex::ObjectRef::new(#bindings),
          ),)*
          _ => ::core::option::Option::None,
        }
      }
    })
    .collect();

  let field_names_arms = data
    .variants
    .iter()
    .map(|variant| {
      let pattern = variant_wildcard(variant);
      let variant_name = reflective_name(&variant.ident);
      let keys = variant
        .fields
        .iter()
        .enumerate()
        .map(|(index, field)| field_key(index, field.ident.as_ref()));
      quote! { #pattern => &[#variant_name, #(#keys),*] }
    })
    .collect();

  let item_arms = data
    .variants
    .iter()
    .map(|variant| match &variant.fields {
      Fields::Unnamed(fields) => {
        let pattern = variant_bindings(variant);
        let indices = 0..fields.unnamed.len();
        let bindings = indices.clone().map(binding);
        quote! {
          #pattern => match __typex_index {
            #(#indices => ::core::option::Option::Some(
              ::typex::ObjectRef::new(#bindings),
            ),)*
            _ => ::core::option::Option::None,
          }
        }
      }
      _ => {
        let pattern = variant_wildcard(variant);
        quote! { #pattern => ::core::option::Option::None }
      }
    })
    .collect();

  let len_arms = data
    .variants
    .iter()
    .map(|variant| {
      let pattern = variant_wildcard(variant);
      match &variant.fields {
        Fields::Unnamed(fields) => {
          let len = fields.unnamed.len();
          quote! { #pattern => ::core::option::Option::Some(#len) }
        }
        _ => quote! { #pattern => ::core::option::Option::None },
      }
    })
    .collect();

  let field_body = match_self(data, field_arms);
  let field_names_body = match_self(data, field_names_arms);
  let item_body = match_self(data, item_arms);
  let len_body = match_self(data, len_arms);

  quote! {
    fn field(
      &self,
      __typex_name: &::core::primitive::str,
    ) -> ::core::option::Option<::typex::ObjectRef<'_>> {
      #field_body
    }

    fn field_names(&self) -> &'static [&'static ::core::primitive::str] {
      #field_names_body
    }

    fn item(
      &self,
      __typex_index: ::core::primitive::usize,
    ) -> ::core::option::Option<::typex::ObjectRef<'_>> {
      #item_body
    }

    fn len(&self) -> ::core::option::Option<::core::primitive::usize> {
      #len_body
    }
  }
}

fn enum_meta_mut(data: &DataEnum) -> TokenStream2 {
  let variant_name_arms = data
    .variants
    .iter()
    .map(|variant| {
      let pattern = variant_wildcard(variant);
      let variant_name = reflective_name(&variant.ident);
      quote! { #pattern => #variant_name }
    })
    .collect();

  let field_arms = data
    .variants
    .iter()
    .map(|variant| {
      let pattern = variant_bindings(variant);
      let keys = variant
        .fields
        .iter()
        .enumerate()
        .map(|(index, field)| field_key(index, field.ident.as_ref()));
      let bindings = (0..variant.fields.len()).map(binding);
      quote! {
        #pattern => match __typex_name {
          #(#keys => ::core::option::Option::Some(
            ::typex::ObjectRefMut::new(#bindings),
          ),)*
          _ => ::core::option::Option::None,
        }
      }
    })
    .collect();

  let item_arms = data
    .variants
    .iter()
    .map(|variant| match &variant.fields {
      Fields::Unnamed(fields) => {
        let pattern = variant_bindings(variant);
        let indices = 0..fields.unnamed.len();
        let bindings = indices.clone().map(binding);
        quote! {
          #pattern => match __typex_index {
            #(#indices => ::core::option::Option::Some(
              ::typex::ObjectRefMut::new(#bindings),
            ),)*
            _ => ::core::option::Option::None,
          }
        }
      }
      _ => {
        let pattern = variant_wildcard(variant);
        quote! { #pattern => ::core::option::Option::None }
      }
    })
    .collect();

  // Resolve the variant name before borrowing fields so that the variant-name
  // result can borrow `self` mutably without overlapping the field borrows.
  let field_body = if data.variants.is_empty() {
    quote! { match *self {} }
  } else {
    let variant_name_body = match_self(data, variant_name_arms);
    let field_match = match_self(data, field_arms);
    quote! {
      let __typex_variant: &'static ::core::primitive::str = #variant_name_body;
      if __typex_name == __typex_variant {
        return ::core::option::Option::Some(
          ::typex::ObjectRefMut::new(self),
        );
      }
      #field_match
    }
  };
  let item_body = match_self(data, item_arms);

  quote! {
    fn field_mut(
      &mut self,
      __typex_name: &::core::primitive::str,
    ) -> ::core::option::Option<::typex::ObjectRefMut<'_>> {
      #field_body
    }

    fn item_mut(
      &mut self,
      __typex_index: ::core::primitive::usize,
    ) -> ::core::option::Option<::typex::ObjectRefMut<'_>> {
      #item_body
    }
  }
}
