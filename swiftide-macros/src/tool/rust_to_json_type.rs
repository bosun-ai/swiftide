use darling::Error;
use syn::{GenericArgument, PathArguments, Type, TypePath};

use super::args::ParamType;

/// Return a best-guess JSON "type" for a given Rust type.
/// For demonstration only—real usage often has a simpler map or uses a dedicated crate.
pub fn rust_type_to_json_type(ty: &Type) -> Result<ParamType, Error> {
    let pty = match ty {
        // Arrays like [T; N]
        Type::Array(_arr) => ParamType::Array,

        // Slices like [T]
        Type::Slice(_slice) => ParamType::Array,

        // Tuples like (A, B, C)
        Type::Tuple(_tuple) => ParamType::Array,

        // &T, &mut T
        Type::Reference(ty_ref) => return rust_type_to_json_type(&ty_ref.elem),

        // Actual paths like `u32`, `String`, `Vec<T>`, `std::collections::HashMap<...>`, etc.
        Type::Path(type_path) => return classify_path(type_path),

        // Parenthesized type `(T)`
        Type::Paren(ty_paren) => return rust_type_to_json_type(&ty_paren.elem),

        // Grouped type `Group`
        Type::Group(ty_group) => return rust_type_to_json_type(&ty_group.elem),

        // Syn may add more variants in the future
        _ => return Err(Error::unsupported_shape("unsupported type")),
    };

    Ok(pty)
}

/// Helper to classify a `Type::Path` (e.g. `u32`, `bool`, `String`, `Vec<T>`, etc.)
fn classify_path(type_path: &TypePath) -> Result<ParamType, Error> {
    // Simple case: if the path is just one segment like `u32`, `String`, `bool`, etc.
    if let Some(ident) = type_path.path.get_ident() {
        return match ident.to_string().as_str() {
            // All the builtin numeric types
            "i8" | "i16" | "i32" | "i64" | "i128" | "u8" | "u16" | "u32" | "u64" | "u128"
            | "isize" | "usize" | "f32" | "f64" => Ok(ParamType::Number),

            "bool" => Ok(ParamType::Boolean),

            "String" | "str" => Ok(ParamType::String),

            // We *could* add object support here
            other => Err(Error::unsupported_shape(&format!(
                "unsupported type {other}"
            ))),
        };
    }

    // If there are multiple segments, e.g. `std::vec::Vec<T>`
    // or `Option<T>`, etc.
    if let Some(last_segment) = type_path.path.segments.last() {
        let seg_str = last_segment.ident.to_string();
        match seg_str.as_str() {
            "Vec" => Ok(ParamType::Array),
            "Option" => {
                // First get the inner type of the Option<T> as a TypePath
                match &last_segment.arguments {
                    PathArguments::AngleBracketed(generics) => {
                        // e.g. Option<T>
                        if let Some(GenericArgument::Type(inner_ty)) = generics.args.first() {
                            // Recursively classify T
                            Some(ParamType::Option(Box::new(rust_type_to_json_type(
                                inner_ty,
                            )?)))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
                .ok_or_else(|| Error::unsupported_shape("unsupported inner option type"))
            }
            // "Option" => {
            //     // This would be very nice to support optional arguments
            //     Err(Error::unsupported_shape("unsupported type"))
            // }
            // For real use, you might handle Result<T, E>, HashMap, etc.
            other => Err(Error::unsupported_shape(&format!(
                "unsupported type {other}"
            ))),
        }
    } else {
        Err(Error::unsupported_shape("unsupported type"))
    }
}
