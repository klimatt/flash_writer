#![no_main]
#![no_std]

use flash_writer;
use flash_writer::flash::FlashWriterError;
use cortex_m_rt::entry;
use cortex_m::asm::delay;
use rtt_target::{rtt_init_print, rprintln};
use core::panic::PanicInfo;
use core::sync::atomic;
use core::sync::atomic::Ordering;

#[entry]
fn main() -> ! {
    rtt_init_print!();
    let mut flash = flash_writer::flash::FlashWriter::new(0x0800_0000u32 + 1024u32 * 5u32 ..=0x0800_0000u32 + 1024u32 * 7u32).unwrap();
    match flash.erase(){
        Ok(_) => { rprintln!("Erase Ok"); }
        Err(e) => { rprintln!("Err: {:?}", e); }
    }
    for i in (0u8..255u8).step_by(7){
        match flash.write(&[i, i+1, i+2, i+3, i+4, i+5, i+6]){
            Ok(_) => { rprintln!("Write Ok"); }
            Err(e) => { rprintln!("Err: {:?}", e); }
        }
    }
    match flash.flush(){
        Ok(_) => { rprintln!("Flush Ok"); }
        Err(e) => { rprintln!("Err: {:?}", e); }
    }
    loop{
        delay(100_000);
    }
}

#[inline(never)]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    rprintln!("Panic");
    loop {
        //atomic::compiler_fence(Ordering::SeqCst);
    }
}

