use proc_macro::TokenStream;
use quote::quote;
use syn::{
    parse_macro_input, Data, DeriveInput, Fields, Type, 
    Attribute, Meta
};

#[proc_macro_derive(CompactSerialize, attributes(max_len_cat, skip))]
pub fn derive_compact_serialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    
    let serialize_body = match &input.data {
        Data::Struct(data) => {
            match &data.fields {
                Fields::Named(fields) => {
                    let field_serializations = fields.named.iter().filter_map(|field| {
                        let field_name = &field.ident;
                        let field_type = &field.ty;
                        
                        // Skip fields with #[skip] attribute
                        if has_skip_attr(&field.attrs) {
                            return None;
                        }
                        
                        Some(if is_vec(field_type) {
                            let max_len = extract_max_len_attr(&field.attrs);
                            match max_len {
                                Some(max_len_val) => {
                                    quote! {
                                        total_bytes += CompactSerializeWithMaxLen::serialize(&self.#field_name, out, crate::ser::MaxLen::#max_len_val)?;
                                    }
                                }
                                None => {
                                    panic!("String and Vec fields must have #[max_len_cat(...)] annotation");
                                }
                            }
                        } else {
                            quote! {
                                total_bytes += self.#field_name.serialize(out)?;
                            }
                        })
                    });
                    
                    quote! {
                        let mut total_bytes = 0;
                        #(#field_serializations)*
                        Ok(total_bytes)
                    }
                }
                _ => panic!("Only structs with named fields are supported"),
            }
        }
        Data::Enum(_) => {
            // Check for #[repr(u8)] attribute
            if !has_repr_u8(&input.attrs) {
                panic!("Enums must have #[repr(u8)] to derive CompactSerialize");
            }
            
            quote! {
                use crate::ser::CompactSerialize;
                (*self as u8).serialize(out)
            }
        }
        _ => panic!("Only structs and enums are supported"),
    };
    
    let expanded = quote! {
        impl crate::ser::CompactSerialize for #name {
            fn serialize<W: std::io::Write>(&self, out: &mut W) -> Result<usize, crate::ser::SerializeError> {
                use crate::ser::CompactSerializeWithMaxLen;
                #serialize_body
            }
        }
    };
    
    TokenStream::from(expanded)
}

fn is_vec(ty: &Type) -> bool {
    match ty {
        Type::Path(type_path) => {
            if let Some(segment) = type_path.path.segments.last() {
                match segment.ident.to_string().as_str() {
                    // "String" => true,
                    "Vec" => true,
                    _ => false,
                }
            } else {
                false
            }
        }
        _ => false,
    }
}

fn extract_max_len_attr(attrs: &[Attribute]) -> Option<syn::Ident> {
    for attr in attrs {
        if attr.path().is_ident("max_len_cat") {
            if let Meta::List(meta_list) = &attr.meta {
                let tokens_str = meta_list.tokens.to_string();
                if let Ok(ident) = syn::parse_str::<syn::Ident>(&tokens_str) {
                    return Some(ident);
                }
            }
        }
    }
    None
}

fn has_repr_u8(attrs: &[Attribute]) -> bool {
    for attr in attrs {
        if attr.path().is_ident("repr") {
            if let Meta::List(meta_list) = &attr.meta {
                let tokens_str = meta_list.tokens.to_string();
                if tokens_str == "u8" {
                    return true;
                }
            }
        }
    }
    false
}

fn has_skip_attr(attrs: &[Attribute]) -> bool {
    for attr in attrs {
        if attr.path().is_ident("skip") {
            return true;
        }
    }
    false
}

#[proc_macro_derive(CompactDeserialize, attributes(max_len_cat, skip))]
pub fn derive_compact_deserialize(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;
    
    let deserialize_body = match &input.data {
        Data::Struct(data) => {
            match &data.fields {
                Fields::Named(fields) => {
                    let field_deserializations = fields.named.iter().filter_map(|field| {
                        let field_name = &field.ident;
                        let field_type = &field.ty;
                        
                        // Skip fields with #[skip] attribute
                        if has_skip_attr(&field.attrs) {
                            return Some(quote! {
                                #field_name: Default::default(),
                            });
                        }
                        
                        Some(if is_vec(field_type) {
                            let max_len = extract_max_len_attr(&field.attrs);
                            match max_len {
                                Some(max_len_val) => {
                                    quote! {
                                        #field_name: crate::de::CompactDeserializeWithMaxLen::deserialize(input, crate::de::MaxLen::#max_len_val)?,
                                    }
                                }
                                None => {
                                    panic!("String and Vec fields must have #[max_len_cat(...)] annotation");
                                }
                            }
                        } else {
                            quote! {
                                #field_name: crate::de::CompactDeserialize::deserialize(input)?,
                            }
                        })
                    });
                    
                    quote! {
                        Ok(#name {
                            #(#field_deserializations)*
                        })
                    }
                }
                _ => panic!("Only structs with named fields are supported"),
            }
        }
        Data::Enum(data) => {
            // Check for #[repr(u8)] attribute
            if !has_repr_u8(&input.attrs) {
                panic!("Enums must have #[repr(u8)] to derive CompactDeserialize");
            }
            
            let variant_matches = data.variants.iter().map(|variant| {
                let variant_name = &variant.ident;
                // Extract discriminant value or use default ordering
                let discriminant_value = if let Some((_, expr)) = &variant.discriminant {
                    quote! { #expr }
                } else {
                    // For enums without explicit discriminants, we can't easily determine the value
                    panic!("Enum variants must have explicit discriminant values for CompactDeserialize");
                };
                
                quote! {
                    #discriminant_value => Ok(Self::#variant_name),
                }
            });
            
            quote! {
                use crate::de::CompactDeserialize;
                let value = u8::deserialize(input)?;
                match value {
                    #(#variant_matches)*
                    _ => Err(crate::de::DeserializeError::InvalidData("Invalid enum value")),
                }
            }
        }
        _ => panic!("Only structs and enums are supported"),
    };
    
    let expanded = quote! {
        impl crate::de::CompactDeserialize for #name {
            fn deserialize<R: std::io::Read>(input: &mut R) -> Result<Self, crate::de::DeserializeError> {
                use crate::de::CompactDeserializeWithMaxLen;
                #deserialize_body
            }
        }
    };
    
    TokenStream::from(expanded)
}
