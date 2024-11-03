mod x;
mod client;

use clang::{Clang, Entity, EntityKind, Index, TypeKind};
use proc_macro2::TokenStream;
use quote::quote;
use std::collections::HashSet;
use std::fs::File;
use std::io::Write;
use syn::{parse2, Item};

// fn main() {
//     let clang = Clang::new().unwrap();
//     let index = Index::new(&clang, false, false);
//     let tu = index.parser("/usr/include/nvml.h").parse().unwrap();
//     let mut functions = tu.get_entity().get_children().into_iter().filter(|e| {
//         e.get_kind() == EntityKind::FunctionDecl
//     }).collect::<Vec<_>>();
//     functions.sort_by(|a, b| a.get_display_name().unwrap().cmp(&b.get_display_name().unwrap()));
//
//     let mut all_types = HashSet::<HashableType>::new();
//
//     for f in functions {
//
//         // println!("{:?}", f.get_comment());
//
//         let return_type = f.get_result_type().unwrap();
//         if return_type.get_kind() != Elaborated && return_type.get_display_name() != "nvmlReturn_t" {
//             continue;
//         }
//         // let children = f.get_children();
//
//         println!("{:?}", f.get_name().unwrap());
//         println!("{:?}", return_type);
//         all_types.insert(HashableType::new(return_type));
//         for c in f.get_children() {
//             if c.get_kind() != EntityKind::ParmDecl {
//                 continue;
//             }
//             println!("  {:?} {:?}", c.get_type().unwrap(), c.get_display_name().unwrap());
//             // println!("  {:?} {}", c.get_type().unwrap(), c.get_display_name().unwrap());
//             all_types.insert(HashableType::new(c.get_type().unwrap()));
//         }
//
//         // let type_child = children.get(0).unwrap();
//         // println!("{:?}", type_child);
//         // if (type_child.get_kind() == EntityKind::ParmDecl) {
//         //  println!("{:?}", f.get_result_type());
//         // println!("{:?}", type_child);
//         //      }
//         //      assert_eq!(type_child.get_kind(), EntityKind::TypeRef);
//         println!();
//     }
//
//     for t in all_types {
//         println!("{:?}", t);
//     }
//
//     let mut enums = tu.get_entity().get_children().into_iter().filter(|e| {
//         e.get_kind() == EntityKind::EnumDecl
//     }).collect::<Vec<_>>();
//     enums.sort_by(|a, b| a.get_display_name().unwrap().cmp(&b.get_display_name().unwrap()));
//     for e in enums {
//         println!("{:?}", e);
//     }
// }

fn main() {
    let clang = Clang::new().unwrap();
    let index = Index::new(&clang, false, false);
    let tu = index.parser("/usr/include/nvml.h").parse().unwrap();
    let required_functions = vec![
        "nvmlInitWithFlags".to_string()
    ];
    let file_content = transform_file(tu.get_entity().get_children(), required_functions);
    let mut file = File::create("parser_wip/src/x.rs").unwrap();
    file.write(file_content.as_bytes()).unwrap();
}

fn transform_file(entities: Vec<Entity>, required_functions: Vec<String>) -> String {
    let (transformed_functions, required_types) =
        transform_required_functions(&entities, &required_functions);

    let enum_tokens = transform_required_enums(&entities, &required_types);

    let final_tokens: Vec<TokenStream> = enum_tokens.into_iter()
        .chain(transformed_functions.into_iter())
        .collect();
    let items: Vec<Item> = final_tokens.into_iter().map(|t| {
        println!("{}", t.to_string());
        parse2::<Item>(t).unwrap()
    }).collect();
    let file = syn::File {
        shebang: None,
        attrs: vec![],
        items,
    };
    prettyplease::unparse(&file)
}

fn transform_required_functions(entities: &[Entity], required_functions: &[String]) -> (Vec<TokenStream>, HashSet<String>) {
    let only_required_functions = entities.iter()
        .filter(|entity| entity.get_kind() == EntityKind::FunctionDecl)
        .filter(|entity| required_functions.contains(&entity.get_name().unwrap()));

    let mut required_types: HashSet<String> = HashSet::<String>::new();

    let result = only_required_functions.map(|entity| {
        let result_type = entity.get_result_type();
        let params = entity.get_children().into_iter()
            .filter(|child| child.get_kind() == EntityKind::ParmDecl)
            .collect::<Vec<_>>();

        // Extract non-trivial types.
        required_types.insert(result_type.unwrap().get_display_name().to_string());
        params.iter()
            .filter(|param| param.get_type().unwrap().get_kind() == TypeKind::Elaborated)
            .for_each(|param| { required_types.insert(param.get_type().unwrap().get_display_name()); });

        let display_name_tok: TokenStream = entity.get_name().unwrap().parse().unwrap();
        let params_toks: Vec<TokenStream> = params.iter().map(|param_entity| {
            assert_eq!(param_entity.get_kind(), EntityKind::ParmDecl);
            let name_tok: TokenStream = param_entity.get_name().unwrap().parse().unwrap();
            let type_tok: TokenStream = match param_entity.get_type().unwrap().get_kind() {
                TypeKind::UInt =>
                    // 32 bits should be enough for `unsigned int` on all realistic platforms
                    quote!(u32),

                t =>
                    panic!("Unsupported type {:?}", t)
            };
            quote! { #name_tok: #type_tok }
        }).collect();

        let result_type_tok: TokenStream = result_type.unwrap().get_display_name().parse().unwrap();
        quote! {
            pub fn #display_name_tok(#(#params_toks),*) -> #result_type_tok {
                let tcp_stream_read = TcpStream::connect("127.0.0.1:19999").unwrap();
            }
        }
    }).collect();
    (result, required_types)
}

fn transform_required_enums(entities: &[Entity], required_types: &HashSet<String>) -> Vec<TokenStream> {
    let only_required_enums = entities.iter()
        .filter(|entity| entity.get_kind() == EntityKind::TypedefDecl)
        .filter(|entity| required_types.contains(&entity.get_display_name().unwrap()));

    only_required_enums.map(|entity| {
        let underlying_type = entity.get_typedef_underlying_type().unwrap();
        assert_eq!(underlying_type.get_kind(), TypeKind::Elaborated);
        let enum_entity = underlying_type.get_declaration().unwrap();
        assert_eq!(enum_entity.get_kind(), EntityKind::EnumDecl);

        let display_name_tok: TokenStream = entity.get_display_name().unwrap().parse().unwrap();
        let decl_toks: Vec<TokenStream> = enum_entity.get_children().iter().map(|decl_entity| {
            assert_eq!(decl_entity.get_kind(), EntityKind::EnumConstantDecl);
            if decl_entity.get_children().len() == 0 {
                decl_entity.get_display_name().unwrap().parse().unwrap()
            } else if decl_entity.get_children().len() == 1 {
                format!("{} = {}",
                        decl_entity.get_display_name().unwrap(),
                        decl_entity.get_enum_constant_value().unwrap().0
                ).parse().unwrap()
            } else {
                panic!("Unexpected number of children in {:?}", entity);
            }
        }).collect();

        quote! {
            pub enum #display_name_tok {
                #(#decl_toks),*
            }
        }
    }).collect::<Vec<_>>()
}
