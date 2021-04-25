use stm32ral::{
    flash,
    read_reg,
    write_reg,
    modify_reg
};
use core::ops::{RangeInclusive,};
use core::borrow::BorrowMut;

/// First and Second keys witch must be written to unlock Flash
const KEY_1: u32 = 0x45670123;
const KEY_2: u32 = 0xCDEF89AB;

#[derive(Debug)]
pub enum FlashWriterError{
    InvalidRange,
    CannotGetFlashRegs,
    BsyTimeout,
    EraseFailed,
    FlashLocked,
    WriteFailed
}

struct WriteBuff {
    data: [u8; FLASH_CFG.program_size],
    len: usize
}

struct FlashConfig {
    addresses: RangeInclusive<u32>,
    banks_amount: u8,
    page_size: usize, // in bytes
    program_size: usize // in bytes
}

pub struct FlashWriter{
    start_address: u32,
    end_address: u32,
    next_write_address: u32,
    buffer: WriteBuff,
    regs: flash::Instance
}

fn check_addresses_range(range: &mut RangeInclusive<u32>) -> bool {
    *FLASH_CFG.addresses.start() <= *range.start() && *FLASH_CFG.addresses.end() >= *range.end()
}


#[link_section = ".data"]
#[inline(never)]
fn check_bsy_sram(regs: &mut flash::Instance) -> Result<(), FlashWriterError> {
    let mut cnt: u8 = 0;
    while read_reg!(flash, regs, SR, BSY) == flash::SR::BSY::R::Active  || cnt < 100 { cnt += 1; }
    match read_reg!(flash, regs, SR, BSY) == flash::SR::BSY::R::Active {
        true => { Err(FlashWriterError::BsyTimeout) }
        false => { Ok(()) }
    }
}

#[link_section = ".data"]
#[inline(never)]
fn erase_sram(flash_writer: &mut FlashWriter) -> Result<(), FlashWriterError> {
    for offset in (flash_writer.start_address..flash_writer.end_address).step_by(FLASH_CFG.page_size) {
        modify_reg!(flash, flash_writer.regs, CR, PER: PageErase);
        #[cfg(feature = "stm32f0x1")]
            write_reg!(flash, flash_writer.regs, AR, offset);
        modify_reg!(flash, flash_writer.regs, CR, STRT: Start);
        match check_bsy_sram(&mut flash_writer.regs) {
            Err(e) => { return Err(e); }
            Ok(_) => {
                match read_reg!(flash, flash_writer.regs, SR, EOP) == flash::SR::EOP::RW::Event {
                    true => { modify_reg!(flash, flash_writer.regs, SR, EOP: Event); }
                    false => { return Err(FlashWriterError::EraseFailed); }
                }
            }
        }
    }
    Ok(())
}

#[link_section = ".data"]
#[inline(never)]
fn write_sram(regs: &mut flash::Instance, address: u32, data: ProgramChunk) -> Result<(), FlashWriterError> {
    modify_reg!(flash, regs, CR, PG: Program);
    let w_a = address as *mut ProgramChunk;
    unsafe { core::ptr::write_volatile(w_a, data) };
    match check_bsy_sram(regs) {
        Err(e) => { return Err(e); }
        Ok(_) => {
            modify_reg!(flash, regs, CR, PG: 0b0);
            match read_reg!(flash, regs, SR, EOP) == flash::SR::EOP::RW::Event {
                true => {
                    modify_reg!(flash, regs, SR, EOP: Event);
                    Ok(())
                }
                false => { return Err(FlashWriterError::WriteFailed); }
            }
        }
    }

}
#[link_section = ".data"]
#[inline(never)]
fn write_all_sram(flash_writer: &mut FlashWriter, data: &[u8]) -> Result<(), FlashWriterError> {
    let mut len_to_take = 0usize;
    if flash_writer.buffer.len != 0 {
        len_to_take = FLASH_CFG.program_size - flash_writer.buffer.len;
        let mut write_buf= [0u8, FLASH_CFG.program_size as u8];
        write_buf.copy_from_slice(&flash_writer.buffer.data[0..flash_writer.buffer.len]);
        write_buf.copy_from_slice(&data[0..len_to_take]);
        let mut dat = 0 as ProgramChunk;
        unsafe {
            core::ptr::copy_nonoverlapping(write_buf.as_ptr(),
                                           &mut dat as *mut _ as *mut u8,
                                           FLASH_CFG.program_size)
        };
        match write_sram(flash_writer.regs.borrow_mut(), flash_writer.next_write_address, dat) {
            Ok(_) => { flash_writer.next_write_address += FLASH_CFG.program_size as u32; }
            Err(e) => { return Err(e); }
        }
    }


    let chunks = data[len_to_take..data.len()].chunks_exact(FLASH_CFG.program_size);
    let remainder = chunks.remainder();

    for bytes in chunks.into_iter(){
        let mut dat = 0 as ProgramChunk;
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(),
                                           &mut dat as *mut _ as *mut u8,
                                           FLASH_CFG.program_size)
        };
        match write_sram(flash_writer.regs.borrow_mut(), flash_writer.next_write_address, dat) {
            Ok(_) => { flash_writer.next_write_address += FLASH_CFG.program_size as u32; }
            Err(e) => { return Err(e); }
        }
    }

    flash_writer.buffer.data.copy_from_slice(remainder);
    flash_writer.buffer.len = remainder.len();
    Ok(())
}

impl FlashWriter{
    pub fn new(mut range: RangeInclusive<u32>) -> Result<self::FlashWriter, FlashWriterError> {
        match check_addresses_range(range.borrow_mut()){
            true => {
                let regs = flash::Flash::take().unwrap(); //TODO remove unwrap
                Ok(
                FlashWriter{
                    start_address: *range.start(),
                    end_address: *range.end(),
                    next_write_address: *range.start(),
                    buffer: WriteBuff{
                        data: [0u8; FLASH_CFG.program_size],
                        len: 0
                    },
                    regs
                })
            }
            false => {Err(FlashWriterError::InvalidRange)}
        }
    }
    pub fn erase(&mut self) -> Result<(), FlashWriterError>{
        match self.unlock(){
            Err(e) => { return Err(e); }
            Ok(_) => {
                match erase_sram(self){
                    Err(e) => { return Err(e); }
                    Ok(_) => {
                        self.lock();
                        Ok(())
                    }
                }
            }
        }

    }

    fn lock(&mut self){
        modify_reg!(flash, self.regs, CR, LOCK: Locked);
    }

    fn unlock(&mut self) -> Result<(), FlashWriterError>{
        match check_bsy_sram(&mut self.regs){
            Err(e) => { return Err(e); }
            Ok(_) => {
                if read_reg!(flash, self.regs, CR, LOCK) == flash::CR::LOCK::RW::Locked {
                    write_reg!(flash, self.regs, KEYR, KEY_1);
                    write_reg!(flash, self.regs, KEYR, KEY_2);
                }
                match read_reg!(flash, self.regs, CR, LOCK) == flash::CR::LOCK::RW::Unlocked {
                    true => Ok(()),
                    false => Err( FlashWriterError::FlashLocked ),
                }
            }
        }
    }

    pub fn write(&mut self, data: &[u8]) -> Result<(), FlashWriterError> {
        match self.unlock(){
            Err(e) => { return Err(e); }
            Ok(_) => {
                match write_all_sram(self, data){
                    Err(e) => { return Err(e); }
                    Ok(_) => {
                        Ok(())
                    }
                }
            }
        }
    }

    pub fn flush(&mut self) -> Result<(),FlashWriterError> {
        if self.buffer.len != 0 {
            let mut dat = ProgramChunk::max_value();
            for i in 0..self.buffer.len{
                dat = dat << 8 | self.buffer.data[i] as ProgramChunk;
            }
            match write_sram(self.regs.borrow_mut(), self.next_write_address, dat){
                Ok(_) => {
                    self.buffer.len = 0;
                    Ok(())
                }
                Err(e) => { return Err(e);}
            }
        }
        else {
            Ok(())
        }
    }
}

#[cfg(feature = "stm32f0x1")]
pub type ProgramChunk = u16;
#[cfg(feature = "stm32f0x1")]
const FLASH_CFG: FlashConfig = FlashConfig{
    page_size: 1024,
    banks_amount: 1,
    addresses: 0x0800_0000..=0x0800_0000 + 512 * 1024,
    program_size: 2
};