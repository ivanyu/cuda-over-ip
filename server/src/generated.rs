use libloading::Library;
use cuda_over_ip_protocol::protocol;
use cuda_over_ip_protocol::protocol::{func_result, func_call, FuncCall, FuncResult};
pub(crate) fn handle_call(
    call: FuncCall,
    libnvidia: &Library,
) -> Result<FuncResult, String> {
    match call.r#type.ok_or("No type provided")? {
        func_call::Type::NvmlInitWithFlagsFuncCall(c) => {
            handle_NvmlInitWithFlags(c, &libnvidia)
        }
        func_call::Type::NvmlShutdownFuncCall(c) => handle_NvmlShutdown(c, &libnvidia),
    }
}
#[allow(non_snake_case)]
fn handle_NvmlInitWithFlags(
    call: protocol::NvmlInitWithFlagsFuncCall,
    libnvidia: &Library,
) -> Result<FuncResult, String> {
    let func: libloading::Symbol<unsafe extern fn(u32) -> i32> = unsafe {
        libnvidia.get(b"nvmlInitWithFlags").unwrap()
    };
    println!("{:?}", func);
    let result = unsafe { func(call.flags) };
    Ok(FuncResult {
        r#type: Some(
            func_result::Type::NvmlInitWithFlagsFuncResult(protocol::NvmlInitWithFlagsFuncResult {
                r#return: result,
            }),
        ),
    })
}
#[allow(non_snake_case)]
fn handle_NvmlShutdown(
    call: protocol::NvmlShutdownFuncCall,
    libnvidia: &Library,
) -> Result<FuncResult, String> {
    let func: libloading::Symbol<unsafe extern fn() -> i32> = unsafe {
        libnvidia.get(b"nvmlShutdown").unwrap()
    };
    println!("{:?}", func);
    let result = unsafe { func() };
    Ok(FuncResult {
        r#type: Some(
            func_result::Type::NvmlShutdownFuncResult(protocol::NvmlShutdownFuncResult {
                r#return: result,
            }),
        ),
    })
}
