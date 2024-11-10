mod generated;

use crate::generated::handle_call;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use cuda_over_ip_protocol::protocol::{FuncCall, FuncResult};
use libloading::Library;
use prost::Message;
use std::io::{BufReader, BufWriter, Read, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use anyhow::Error;
use cuda_over_ip_common::RPC;

fn main() {
    let listener = TcpListener::bind("127.0.0.1:19999").unwrap();

    while let Ok((tcp_stream_read, client)) = listener.accept() {
        println!("Client {} connected", client);
        tcp_stream_read.set_nodelay(true).expect("set_nodelay call failed");
        thread::spawn(move || {
            serve(tcp_stream_read);
        });
    }
}

fn serve(tcp_stream_read: TcpStream) {
    let libcuda = unsafe { Library::new("libcuda.so.1").unwrap() };

    let tcp_stream_write = tcp_stream_read.try_clone().unwrap();
    let mut buf_writer: BufWriter<TcpStream> = BufWriter::new(tcp_stream_write);
    let mut buf_reader: BufReader<TcpStream> = BufReader::new(tcp_stream_read);

    loop {
        let result = serve_iteration(&mut buf_writer, &mut buf_reader, &libcuda);
        if let Err(e) = &result {
            match e.root_cause().downcast_ref::<std::io::Error>() {
                Some(rc) if rc.kind() == std::io::ErrorKind::UnexpectedEof => {
                    println!("Client disconnected");
                    break;
                }

                _ => {
                    result.unwrap();
                }
            }
        } else {
            result.unwrap();
        }
    }
}

fn serve_iteration(buf_writer: &mut BufWriter<TcpStream>,
                   buf_reader: &mut BufReader<TcpStream>,
                   libcuda: &Library) -> anyhow::Result<()> {
    let rpc_id = buf_reader.read_i32::<BigEndian>()?;
    let rpc = RPC::parse(rpc_id);
    match rpc {
        RPC::cuDriverGetVersion => handle_cuDriverGetVersion(buf_writer, buf_reader, &libcuda),
        _ => Err(Error::msg(format!("Unsupported RPC {:?}", rpc))),
    }
}

fn handle_cuDriverGetVersion(buf_writer: &mut BufWriter<TcpStream>,
                             buf_reader: &mut BufReader<TcpStream>,
                             libcuda: &Library) -> anyhow::Result<()> {
    let func: libloading::Symbol<unsafe extern fn(*mut i32) -> i32> = unsafe {
        libcuda.get(b"cuDriverGetVersion")?
    };

    let mut driverVersion_vec = vec![0_u8; size_of::<i32>()];
    buf_reader.read_exact(&mut driverVersion_vec)?;
    let driverVersion = driverVersion_vec.as_mut_ptr() as *mut i32;

    let result: i32 = unsafe { func(driverVersion) };

    buf_writer.write_i32::<BigEndian>(result)?;
    buf_writer.write(&driverVersion_vec)?;
    buf_writer.flush()?;

    Ok(())
}


// fn serve(tcp_stream_read: TcpStream) {
//     let libnvidia = unsafe { Library::new("libnvidia-ml.so.1").unwrap() };
//
//     let tcp_stream_write = tcp_stream_read.try_clone().unwrap();
//     let mut buf_writer: BufWriter<TcpStream> = BufWriter::new(tcp_stream_write);
//     let mut buf_reader: BufReader<TcpStream> = BufReader::new(tcp_stream_read);
//
//     loop {
//         let call = match read_call(&mut buf_reader) {
//             Ok(c) => c,
//             Err(e) => {
//                 eprintln!("Error reading call: {}", e);
//                 break;
//             }
//         };
//         println!("{:?}", call);
//
//         let result = match handle_call(call, &libnvidia) {
//             Ok(r) => r,
//             Err(e) => {
//                 eprintln!("Error handling call: {}", e);
//                 break;
//             }
//         };
//         println!("{:?}", result);
//
//         if let Err(e) = send_result(&mut buf_writer, result) {
//             eprintln!("Error responding: {}", e);
//             break;
//         }
//     }
// }
//
// fn read_call(buf_reader: &mut BufReader<TcpStream>) -> std::io::Result<FuncCall> {
//     let size = buf_reader.read_u32::<BigEndian>()? as usize;
//     let mut buf = vec![0_u8; size];
//     buf_reader.read_exact(&mut buf)?;
//     Ok(FuncCall::decode(&*buf)?)
// }
//
// fn send_result(buf_writer: &mut BufWriter<TcpStream>, result: FuncResult) -> std::io::Result<()> {
//     let mut buf = Vec::<u8>::with_capacity(result.encoded_len());
//     result.encode(&mut buf)?;
//     buf_writer.write_u32::<BigEndian>(result.encoded_len() as u32)?;
//     buf_writer.write_all(&buf)?;
//     buf_writer.flush()
// }
