use clang::{Clang, Entity, EntityKind, Index, Type, TypeKind};
use proc_macro2::TokenStream;
use quote::quote;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use heck::AsUpperCamelCase;
use serde::Deserialize;
use syn::{parse2, Item};
use syn::parse::Parser;

const RETURN_FIELD: &str = "return";

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

#[derive(Debug, Deserialize)]
#[derive(PartialEq)]
enum ParameterDirection {
    #[serde(rename(deserialize = "in"))]
    IN,
    #[serde(rename(deserialize = "out"))]
    OUT,
}

#[derive(Debug, Deserialize)]
struct FunctionParameter {
    name: String,
    direction: ParameterDirection,
}

#[derive(Debug, Deserialize)]
struct FunctionDescription {
    name: String,
    params: Vec<FunctionParameter>,
}

impl FunctionDescription {
    fn get_param(&self, name: &str) -> &FunctionParameter {
        self.params.iter().find(|p| p.name == name).unwrap()
    }
}

#[derive(Debug)]
struct CFunction<'a> {
    return_type: Type<'a>,
    params: Vec<Entity<'a>>,
}

#[derive(Debug)]
struct FunctionDef<'a> {
    description: FunctionDescription,
    c_function: CFunction<'a>,
}

impl FunctionDef<'_> {
    fn required_nontrivial_types(&self) -> HashSet<String> {
        let mut required_types = HashSet::new();
        required_types.insert(self.c_function.return_type.get_display_name().to_string());
        self.c_function.params.iter()
            .filter(|param| param.get_type().unwrap().get_kind() == TypeKind::Elaborated)
            .for_each(|param| { required_types.insert(param.get_type().unwrap().get_display_name()); });
        required_types
    }
}

#[derive(Debug)]
struct EnumDef<'a> {
    name: String,
    elements: Vec<Entity<'a>>,
}

fn main() {
    let input_file = File::open("parser_wip/functions.yaml").unwrap();
    let function_descriptions: Vec<FunctionDescription> = serde_yaml_ng::from_reader(input_file).unwrap();

    let clang = Clang::new().unwrap();
    let index = Index::new(&clang, false, false);
    let tu = index.parser("/usr/include/nvml.h").parse().unwrap();
    let entities = tu.get_entity().get_children();

    // let file_content = transform_file(tu.get_entity().get_children(), function_descriptions);
    // let mut file = File::create("parser_wip/src/x.rs").unwrap();
    // file.write(file_content.as_bytes()).unwrap();

    let mut c_functions = extract_functions(
        &entities,
        function_descriptions.iter().map(|fd| fd.name.clone()).collect(),
    );
    let functions: HashMap<String, FunctionDef> = function_descriptions.into_iter().map(|fd| {
        let function_name = fd.name.clone();
        let full_function_def = FunctionDef { description: fd, c_function: c_functions.remove(&function_name).unwrap() };
        (function_name.clone(), full_function_def)
    }).collect();

    let enums = extract_enums(&entities, &functions);

    generate_protobuf("protocol/src/protocol.proto", &enums, &functions);
    generate_client("client/src/lib.rs", &enums, &functions);
    generate_server("server/src/generated.rs", &enums, &functions);
}

fn extract_functions<'a>(entities: &'a [Entity<'a>], required_functions: HashSet<String>) -> HashMap<String, CFunction<'a>> {
    let only_required_functions = entities.iter()
        .filter(|entity| entity.get_kind() == EntityKind::FunctionDecl)
        .filter(|entity| required_functions.contains(&entity.get_name().unwrap()));

    only_required_functions.map(|entity| {
        let return_type = entity.get_result_type().unwrap();
        let params = entity.get_children().into_iter()
            .filter(|child| child.get_kind() == EntityKind::ParmDecl)
            .collect::<Vec<_>>();

        (entity.get_name().unwrap(), CFunction { return_type, params })
    }).collect::<HashMap<_, _>>()
}

fn extract_enums<'a>(entities: &'a [Entity<'a>], functions: &'a HashMap<String, FunctionDef<'a>>) -> Vec<EnumDef<'a>> {
    let required_types: Vec<String> = functions.values()
        .flat_map(|f| f.required_nontrivial_types()).collect();

    let only_required_enums = entities.iter()
        .filter(|entity| entity.get_kind() == EntityKind::TypedefDecl)
        .filter(|entity| required_types.contains(&entity.get_display_name().unwrap()));

    only_required_enums.map(|entity| {
        let underlying_type = entity.get_typedef_underlying_type().unwrap();
        assert_eq!(underlying_type.get_kind(), TypeKind::Elaborated);
        let enum_entity = underlying_type.get_declaration().unwrap();
        assert_eq!(enum_entity.get_kind(), EntityKind::EnumDecl);

        let elements: Vec<Entity<'a>> = enum_entity.get_children().into_iter().map(|decl_entity| {
            assert_eq!(decl_entity.get_kind(), EntityKind::EnumConstantDecl);
            assert!(decl_entity.get_children().len() == 0 || decl_entity.get_children().len() == 1);
            decl_entity
        }).collect();

        EnumDef {
            name: entity.get_display_name().unwrap(),
            elements,
        }
    }).collect::<Vec<_>>()
}

fn generate_protobuf(output_path: &str, enums: &[EnumDef], functions: &HashMap<String, FunctionDef>) {
    let header_proto = generate_protobuf_header();
    let enums_proto = generate_protobuf_enums(&enums);
    let functions_proto = generate_protobuf_functions(&functions);
    let mut output_file = File::create(output_path).unwrap();
    output_file.write(header_proto.as_bytes()).unwrap();
    output_file.write(enums_proto.as_bytes()).unwrap();
    output_file.write(functions_proto.as_bytes()).unwrap();
}

fn generate_protobuf_header() -> String {
    let mut result = String::new();

    result.push_str("syntax = \"proto3\";\n");
    result.push_str("\n");

    result.push_str("package protocol;\n");
    result.push_str("\n");

    result
}

fn generate_protobuf_enums(enums: &[EnumDef]) -> String {
    let mut result = String::new();

    for e in enums {
        result.push_str(&format!("enum {} {{\n", e.name));
        for element in e.elements.iter() {
            match element.get_children()[..] {
                [] => {
                    result.push_str(&format!("  {};\n", element.get_display_name().unwrap()));
                }
                [_] => {
                    result.push_str(&format!("  {} = {};\n",
                                             element.get_display_name().unwrap(),
                                             element.get_enum_constant_value().unwrap().0));
                }
                _ => panic!("shouldn't be here")
            }
        }
        result.push_str("}\n");
    }
    result
}

fn generate_protobuf_functions(functions: &HashMap<String, FunctionDef>) -> String {
    let mut result = String::new();

    for (name, func_def) in functions {
        result.push_str(&format!("message {}FuncCall {{\n", name));

        for (i, param) in func_def.c_function.params.iter().enumerate() {
            let param_name = param.get_name().unwrap();
            let peram = func_def.description.get_param(&param_name);
            if peram.direction == ParameterDirection::IN {
                result.push_str(
                    &format!("  {} {} = {};\n",
                             c_to_protobuf_type(&param.get_type().unwrap()),
                             param_name,
                             i + 1));
            }
        }

        result.push_str("}\n");
        result.push_str("\n");

        result.push_str(&format!("message {}FuncResult {{\n", name));

        result.push_str(
            &format!("  {} {} = {};\n",
                     c_to_protobuf_type(&func_def.c_function.return_type),
                     RETURN_FIELD,
                     1));
        result.push_str("}\n");
    }
    result.push_str("\n");

    result.push_str("message FuncCall {\n");
    result.push_str("  oneof type {\n");
    for (i, (name, _)) in functions.iter().enumerate() {
        result.push_str(&format!("    {}FuncCall {}FuncCall = {};\n", name, name, i + 1));
    }
    result.push_str("  }\n");
    result.push_str("}\n");
    result.push_str("\n");

    result.push_str("message FuncResult {\n");
    result.push_str("  oneof type {\n");
    for (i, (name, _)) in functions.iter().enumerate() {
        result.push_str(&format!("    {}FuncResult {}FuncResult = {};\n", name, name, i + 1));
    }
    result.push_str("  }\n");
    result.push_str("}\n");

    result
}

fn c_to_protobuf_type(type_: &Type) -> String {
    match type_.get_kind() {
        TypeKind::UInt => "uint32".to_string(),
        TypeKind::Elaborated => type_.get_display_name(),

        _ => panic!("Unsupported type {:?}", type_)
    }
}

fn generate_client(output_path: &str, _enums: &[EnumDef], functions: &HashMap<String, FunctionDef>) {
    let other_imports = vec![
        quote! {use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};},
        quote! {use std::io::{BufReader, BufWriter, Read, Write};},
        quote! {use std::net::TcpStream;},
        quote! {use prost::Message;},
    ];

    let mut protocol_imports: HashSet<String> = HashSet::new();
    protocol_imports.insert("FuncCall".to_owned());
    protocol_imports.insert("func_call".to_owned());
    protocol_imports.insert("FuncResult".to_owned());
    protocol_imports.insert("func_result".to_owned());

    let functions_tok: Vec<TokenStream> = functions.iter().map(|(name, def)| {
        let name_tok: TokenStream = name.parse().unwrap();
        let params_tok: Vec<TokenStream> = def.c_function.params.iter().map(|param_entity| {
            assert_eq!(param_entity.get_kind(), EntityKind::ParmDecl);
            let name_tok: TokenStream = param_entity.get_name().unwrap().parse().unwrap();
            let type_tok: TokenStream = c_type_to_rust(&param_entity.get_type().unwrap());
            quote! { #name_tok: #type_tok }
        }).collect();
        let rust_enum_name = c_to_rust_name(&def.c_function.return_type.get_display_name());
        let result_type_tok: TokenStream = rust_enum_name.parse().unwrap();
        protocol_imports.insert(rust_enum_name.clone());

        let call_struct_name = c_to_rust_name(name) + "FuncCall";
        let call_struct_name_tok: TokenStream = call_struct_name.parse().unwrap();
        protocol_imports.insert(call_struct_name);
        let result_struct_name = c_to_rust_name(name) + "FuncResult";
        let result_struct_name_tok: TokenStream = result_struct_name.parse().unwrap();
        protocol_imports.insert(result_struct_name);

        let in_params_tok: Vec<TokenStream> = def.description.params.iter()
            .filter(|param| param.direction == ParameterDirection::IN)
            .map(|param| param.name.parse().unwrap())
            .collect();
        let out_params_tok: Vec<TokenStream> = def.description.params.iter()
            .filter(|param| param.direction == ParameterDirection::OUT)
            .map(|param| param.name.parse().unwrap())
            .collect();

        quote! {
            #[no_mangle]
            pub unsafe extern "C" fn #name_tok (#(#params_tok),*) -> #result_type_tok {
                let tcp_stream_read = TcpStream::connect("127.0.0.1:19999").unwrap();
                let tcp_stream_write = tcp_stream_read.try_clone().unwrap();
                tcp_stream_read.set_nodelay(true).unwrap();

                let call = FuncCall {
                    r#type: Some(
                        func_call::Type::#call_struct_name_tok(#call_struct_name_tok { #(#in_params_tok),* })
                    )
                };
                let mut buf = Vec::<u8>::with_capacity(call.encoded_len());
                call.encode(&mut buf).unwrap();

                let mut buf_writer = BufWriter::new(tcp_stream_write);
                buf_writer.write_u32::<BigEndian>(call.encoded_len() as u32).unwrap();
                buf_writer.write_all(&buf).unwrap();
                buf_writer.flush().unwrap();

                let mut buf_reader = BufReader::new(tcp_stream_read);
                let size = buf_reader.read_u32::<BigEndian>().unwrap() as usize;
                let mut buf = vec![0_u8; size];
                buf_reader.read_exact(&mut buf).unwrap();
                let result = FuncResult::decode(&*buf).unwrap();
                match result.r#type {
                    Some(func_result::Type::#result_struct_name_tok(
                        #(#out_params_tok),* #result_struct_name_tok{r#return}
                    )) => {
                        println!("{:?}", result);
                        #result_type_tok::try_from(r#return).unwrap()
                    },
                    None => panic!("Invalid type")
                }
            }
        }
    }).collect();

    let protocol_imports_tok: Vec<TokenStream> =
        protocol_imports.iter().map(|name| name.parse().unwrap()).collect();
    let imports_tok = vec![
        quote! {
            use cuda_over_ip_protocol::protocol::{ #(#protocol_imports_tok),* };
        },
    ];

    let final_tokens: Vec<TokenStream> = other_imports.into_iter()
        .chain(imports_tok.into_iter())
        .chain(functions_tok.into_iter())
        .collect();
    let items: Vec<Item> = final_tokens.into_iter()
        .map(|t| parse2::<Item>(t).unwrap()).collect();
    let file = syn::File {
        shebang: None,
        attrs: vec![],
        items,
    };
    let text = prettyplease::unparse(&file);

    let mut output_file = File::create(output_path).unwrap();
    output_file.write(text.as_bytes()).unwrap();
}

fn generate_server(output_path: &str, _enums: &[EnumDef], functions: &HashMap<String, FunctionDef>) {
    let other_imports = vec![
        quote! {use libloading::Library;},
        quote! {use cuda_over_ip_protocol::protocol;},
        quote! {use cuda_over_ip_protocol::protocol::{func_result, func_call, FuncCall, FuncResult};},
    ];

    // let mut protocol_imports: HashSet<String> = HashSet::new();

    let handle_call_tok = generate_high_level_handle_call_function(&functions);
    let functions_tok: Vec<TokenStream> = functions.iter().map(|(name, function_def)| {
        generate_concrete_handle_call_function(&name, &function_def)
    }).collect();

    let final_tokens: Vec<TokenStream> = other_imports.into_iter()
        // .chain(imports_tok.into_iter())
        .chain(vec![handle_call_tok].into_iter())
        .chain(functions_tok.into_iter())
        .collect();
    let items: Vec<Item> = final_tokens.into_iter()
        .map(|t| parse2::<Item>(t).unwrap()).collect();
    let file = syn::File {
        shebang: None,
        attrs: vec![],
        items,
    };
    let text = prettyplease::unparse(&file);

    let mut output_file = File::create(output_path).unwrap();
    output_file.write(text.as_bytes()).unwrap();
}

fn generate_high_level_handle_call_function(functions: &HashMap<String, FunctionDef>) -> TokenStream {
    let match_branch_toks: Vec<TokenStream> = functions.iter().map(|(name, function_def)| {
        let call_struct_name = c_to_rust_name(name) + "FuncCall";
        let call_struct_name_tok: TokenStream = call_struct_name.parse().unwrap();
        let handle_function_name = format!("handle_{}", c_to_rust_name(name));
        let handle_function_name_tok: TokenStream = handle_function_name.parse().unwrap();

        quote! {
            func_call::Type::#call_struct_name_tok(c) => #handle_function_name_tok(c, &libnvidia)
        }
    }).collect();

    quote! {
        pub(crate) fn handle_call(call: FuncCall, libnvidia: &Library) -> Result<FuncResult, String> {
            match call.r#type.ok_or("No type provided")? {
                #(#match_branch_toks),*
            }
        }
    }
}

fn generate_concrete_handle_call_function(name: &String, function_def: &FunctionDef) -> TokenStream {
    let c_func_name_bytes = format!("b\"{}\"", name);
    let c_func_name_bytes_tok: TokenStream = c_func_name_bytes.parse().unwrap();

    let symbol_param_toks: Vec<TokenStream> = function_def.c_function.params.iter().map(|param| {
        c_type_to_rust(&param.get_type().unwrap())
    }).collect();
    let symbol_result_tok = c_type_to_rust(&function_def.c_function.return_type);
    let symbol_tok = quote! {
        libloading::Symbol<unsafe extern fn(#(#symbol_param_toks),*) -> #symbol_result_tok>
    };

    let call_param_toks: Vec<TokenStream> = function_def.c_function.params.iter().map(|param| {
        let name_tok: TokenStream = param.get_name().unwrap().parse().unwrap();
        quote! {call.#name_tok}
    }).collect();

    let result_struct_name = c_to_rust_name(name) + "FuncResult";
    let result_struct_name_tok: TokenStream = result_struct_name.parse().unwrap();

    quote! {
        fn handle_NvmlInitWithFlags(call: protocol::NvmlInitWithFlagsFuncCall, libnvidia: &Library) -> Result<FuncResult, String> {
            let func: #symbol_tok = unsafe {
                libnvidia.get(#c_func_name_bytes_tok).unwrap()
            };
            println!("{:?}", func);
            let result = unsafe { func(#(#call_param_toks),*) };

            Ok(FuncResult {
                r#type: Some(
                    func_result::Type::#result_struct_name_tok(protocol::#result_struct_name_tok {
                        r#return: result
                    })
                ),
            })
        }
    }
}

fn c_to_rust_name(c_enum_name: &str) -> String {
    AsUpperCamelCase(c_enum_name).to_string()
}

fn c_type_to_rust(type_: &Type) -> TokenStream {
    match type_.get_kind() {
        TypeKind::UInt =>
        // 32 bits should be enough for `unsigned int` on all realistic platforms.
            quote!(u32),

        TypeKind::Elaborated =>
        // Maybe at some point we need something more sophisticated, but now assuming this is struct.
            quote!(i32),

        t =>
            panic!("Unsupported type {:?}", t)
    }
}

/*fn transform_file(entities: Vec<Entity>, function_descriptions: HashMap<String, FunctionDescription>) -> String {
    let imports = vec![
        quote! {use cuda_over_ip_protocol::protocol_calls;},
        quote! {use cuda_over_ip_protocol::protocol_calls::{remote_call, RemoteCall};},
    ];

    let (transformed_functions, required_types) =
        transform_required_functions(&entities, &function_descriptions);

    let enum_tokens = transform_required_enums(&entities, &required_types);

    let final_tokens: Vec<TokenStream> = imports.into_iter()
        .chain(enum_tokens.into_iter())
        .chain(transformed_functions.into_iter())
        .collect();
    let items: Vec<Item> = final_tokens.into_iter()
        .map(|t| parse2::<Item>(t).unwrap()).collect();
    let file = syn::File {
        shebang: None,
        attrs: vec![],
        items,
    };
    prettyplease::unparse(&file)
}

fn transform_required_functions(entities: &[Entity], required_functions: &HashMap<String, FunctionDescription>) -> (Vec<TokenStream>, HashSet<String>) {
    let only_required_functions = entities.iter()
        .filter(|entity| entity.get_kind() == EntityKind::FunctionDecl)
        .filter(|entity| required_functions.contains_key(&entity.get_name().unwrap()));

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

        let message_name_tok: TokenStream = heck::AsUpperCamelCase(entity.get_name().unwrap()).to_string()
            .parse().unwrap();
        let result_type_tok: TokenStream = result_type.unwrap().get_display_name().parse().unwrap();
        quote! {
            pub fn #display_name_tok(#(#params_toks),*) -> #result_type_tok {
                let call = RemoteCall {
                    call: Some(
                        remote_call::Call::#message_name_tok (
                            protocol_calls::#message_name_tok {
                                flags: flags
                            }
                        )
                    )
                };
                nvmlReturn_t::NVML_ERROR_UNKNOWN
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
*/