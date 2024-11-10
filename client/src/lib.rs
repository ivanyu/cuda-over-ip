mod non_generated;

use std::ffi::c_void;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use std::io::{BufReader, BufWriter, Read, Write};
use crate::non_generated::ptr_as_u8_slice;
use cuda_over_ip_common::RPC;

#[no_mangle]
pub unsafe extern "C" fn cuDriverGetVersion(driverVersion: *mut i32) -> i32 {
    let mut write_guard = non_generated::WRITER_AND_READER.write();
    let (buf_writer, buf_reader) = match write_guard.get_mut() {
        Ok(r) => r,
        Err(_) => panic!("poisoned"),
    };

    buf_writer.write_i32::<BigEndian>(RPC::cuDriverGetVersion as i32).unwrap();

    let mut driverVersion_slice = ptr_as_u8_slice(driverVersion);
    buf_writer.write_all(driverVersion_slice).unwrap();

    buf_writer.flush().unwrap();

    let result = buf_reader.read_i32::<BigEndian>().unwrap();

    buf_reader.read_exact(&mut driverVersion_slice).unwrap();

    result
}

// #[no_mangle]
// pub unsafe extern "C" fn nvmlInitWithFlags(flags: u32) -> protocol::NvmlReturnT {
//     let mut write_guard = non_generated::WRITER_AND_READER.write();
//     let (buf_writer, buf_reader) = match write_guard.get_mut() {
//         Ok(r) => r,
//         Err(_) => panic!("poisoned"),
//     };
//     let mut in_slices: Vec<IoSlice> = Vec::new();
//     in_slices.push(IoSlice::new(as_u8_slice(&0_u32)));
//     in_slices.push(IoSlice::new(as_u8_slice(&flags)));
//     send_call(buf_writer, in_slices);
//     let mut out_slices: Vec<IoSliceMut> = Vec::new();
//     let mut result_vec = vec![0_u8; 4];
//     out_slices.push(IoSliceMut::new(result_vec.as_mut_slice()));
//     read_result(buf_reader, out_slices);
//     let result: i32 = u8_slice_as_value(&result_vec);
//     protocol::NvmlReturnT::try_from(result).unwrap()
// }
//
// #[no_mangle]
// pub unsafe extern "C" fn nvmlShutdown() -> protocol::NvmlReturnT {
//     let mut write_guard = non_generated::WRITER_AND_READER.write();
//     let (buf_writer, buf_reader) = match write_guard.get_mut() {
//         Ok(r) => r,
//         Err(_) => panic!("poisoned"),
//     };
//     let mut in_slices: Vec<IoSlice> = Vec::new();
//     in_slices.push(IoSlice::new(as_u8_slice(&1_u32)));
//     send_call(buf_writer, in_slices);
//     let mut out_slices: Vec<IoSliceMut> = Vec::new();
//     let mut result_vec = vec![0_u8; 4];
//     out_slices.push(IoSliceMut::new(result_vec.as_mut_slice()));
//     read_result(buf_reader, out_slices);
//     let result: i32 = u8_slice_as_value(&result_vec);
//     protocol::NvmlReturnT::try_from(result).unwrap()
// }
