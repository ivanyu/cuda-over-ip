use clang::{Clang, Entity, EntityKind, Index, Type, TypeKind};
use proc_macro2::TokenStream;
use quote::quote;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Write;
use heck::AsUpperCamelCase;
use serde::Deserialize;
use syn::{parse2, Item};

const RETURN_FIELD: &str = "return";

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
    id: u32,
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
    check_id_uniqueness(&function_descriptions);

    let clang = Clang::new().unwrap();
    let index = Index::new(&clang, false, false);
    let tu = index.parser("/usr/include/nvml.h").parse().unwrap();
    let entities = tu.get_entity().get_children();

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

    // generate_protobuf("protocol/src/protocol.proto", &enums, &functions);
    generate_client("client/src/lib.rs", &enums, &functions);
    generate_server("server/src/generated.rs", &enums, &functions);
}

fn check_id_uniqueness(function_descriptions: &Vec<FunctionDescription>) {
    let mut seen_ids: HashSet<u32> = HashSet::new();
    for fd in function_descriptions {
        if !seen_ids.insert(fd.id) {
            panic!("Duplicate function ID {}", fd.id);
        }
    }
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
    let mod_tok = quote! {mod non_generated;};

    let import_toks = vec![
        quote! {use std::io::{IoSlice, IoSliceMut};},
        quote! {use byteorder::ReadBytesExt;},
        quote! {use cuda_over_ip_protocol::protocol;},
        quote! {use cuda_over_ip_protocol::protocol::{func_result, func_call, FuncCall};},
        quote! {use crate::non_generated::{as_u8_slice, u8_slice_as_value, send_call, read_result};},
    ];

    let function_toks: Vec<TokenStream> = functions.iter().map(|(name, def)| {
        let name_tok: TokenStream = name.parse().unwrap();
        let id_tok: TokenStream = format!("{}_u32", def.description.id).parse().unwrap();
        let params_tok: Vec<TokenStream> = def.c_function.params.iter().map(|param_entity| {
            assert_eq!(param_entity.get_kind(), EntityKind::ParmDecl);
            let name_tok: TokenStream = param_entity.get_name().unwrap().parse().unwrap();
            let type_tok: TokenStream = c_type_to_rust(&param_entity.get_type().unwrap());
            quote! { #name_tok: #type_tok }
        }).collect();
        let rust_enum_name = c_to_rust_name(&def.c_function.return_type.get_display_name());
        let result_type_tok: TokenStream = rust_enum_name.parse().unwrap();

        let call_struct_name = c_to_rust_name(name) + "FuncCall";
        let call_struct_name_tok: TokenStream = call_struct_name.parse().unwrap();
        let result_struct_name = c_to_rust_name(name) + "FuncResult";
        let result_struct_name_tok: TokenStream = result_struct_name.parse().unwrap();

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
            pub unsafe extern "C" fn #name_tok (#(#params_tok),*) -> protocol:: #result_type_tok {
                let mut write_guard = non_generated::WRITER_AND_READER.write();
                let (buf_writer, buf_reader) = match write_guard.get_mut() {
                    Ok(r) => { r }
                    Err(_) => panic!("poisoned")
                };

                let mut in_slices: Vec<IoSlice> = Vec::new();
                in_slices.push(IoSlice::new(as_u8_slice(& #id_tok)));
                #(in_slices.push(IoSlice::new(as_u8_slice(& #in_params_tok)));)*
                send_call(buf_writer, in_slices);

                let mut out_slices: Vec<IoSliceMut> = Vec::new();
                let mut result_vec = vec![0_u8; 4];
                out_slices.push(IoSliceMut::new(result_vec.as_mut_slice()));
                read_result(buf_reader, out_slices);
                let result: i32 = u8_slice_as_value(&result_vec);
                protocol::NvmlReturnT::try_from(result).unwrap()
                // let result: protocol:: #result_type_tok = buf_reader.read_u32()
                //     .and_then(protocol:: #result_type_tok::try_from)
                //     .unwrap();
                // result
                // let call = FuncCall {
                //     r#type: Some(
                //         func_call::Type::#call_struct_name_tok(protocol:: #call_struct_name_tok { #(#in_params_tok),* })
                //     )
                // };
                // let result = send_call_and_get_result(buf_writer, buf_reader, call);
                //
                // match result.r#type {
                //     Some(func_result::Type::#result_struct_name_tok(
                //         #(#out_params_tok),* protocol:: #result_struct_name_tok{r#return}
                //     )) => {
                //         println!("{:?}", result);
                //         protocol:: #result_type_tok::try_from(r#return).unwrap()
                //     },
                //     t => panic!("Invalid type {:?}", t)
                // }
            }
        }
    }).collect();

    let final_tokens: Vec<TokenStream> = vec![mod_tok].into_iter()
        .chain(import_toks.into_iter())
        .chain(function_toks.into_iter())
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

    let handle_call_tok = generate_high_level_handle_call_function(&functions);
    let functions_tok: Vec<TokenStream> = functions.iter().map(|(name, function_def)| {
        generate_concrete_handle_call_function(&name, &function_def)
    }).collect();

    let final_tokens: Vec<TokenStream> = other_imports.into_iter()
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
    let match_branch_toks: Vec<TokenStream> = functions.iter().map(|(name, _)| {
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
    let handle_function_name = format!("handle_{}", c_to_rust_name(name));
    let handle_function_name_tok: TokenStream = handle_function_name.parse().unwrap();

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

    let call_struct_name = c_to_rust_name(name) + "FuncCall";
    let call_struct_name_tok: TokenStream = call_struct_name.parse().unwrap();
    let result_struct_name = c_to_rust_name(name) + "FuncResult";
    let result_struct_name_tok: TokenStream = result_struct_name.parse().unwrap();

    quote! {
        #[allow(non_snake_case)]
        fn #handle_function_name_tok(call: protocol:: #call_struct_name_tok, libnvidia: &Library) -> Result<FuncResult, String> {
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
