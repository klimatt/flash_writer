use core::ops::RangeInclusive;
use core::borrow::BorrowMut;
use stm32_device_signature;
use cfg_if::cfg_if;

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
    WriteFailed,
    WrongBankId,
    OutOfFlashWriterMemory,
    ProgErr,
    SizeErr,
    PgaErr,
    PgsErr,
    WrpErr,
    MissErr,
    FastErr,
}

struct WriteBuff {
    data: [u8; PROGRAM_SIZE],
    len: usize
}

pub struct FlashWriter{
    bank_change_on_page_num: u32,
    start_address: u32,
    end_address: u32,
    next_write_address: u32,
    image_len: usize,
    buffer: WriteBuff,
    regs: FLASH
}

fn check_range(range_cont: &mut RangeInclusive<u32>, range_check: &mut RangeInclusive<u32>) -> bool {
    range_cont.contains(range_check.start()) && range_cont.contains(range_check.end())
}

#[link_section = ".data"]
#[inline(never)]
fn check_errors_ram(regs: &mut FLASH) -> Result<(), FlashWriterError> {
    let sr = regs.sr.read();
    cfg_if! {
        if #[cfg(feature = "stm32f0xx")] {
            if sr.pgerr().bit_is_set() { return Err(FlashWriterError::ProgErr); }
            if sr.wrprt().bit_is_set() { return Err(FlashWriterError::WrpErr); }
        }
        else{
            if sr.progerr().bit_is_set() { return Err(FlashWriterError::ProgErr); }
            if sr.sizerr().bit_is_set() { return Err(FlashWriterError::SizeErr); }
            if sr.pgaerr().bit_is_set() { return Err(FlashWriterError::PgaErr); }
            if sr.pgserr().bit_is_set() { return Err(FlashWriterError::PgsErr); }
            if sr.wrperr().bit_is_set() { return Err(FlashWriterError::WrpErr); }
            if sr.miserr().bit_is_set() { return Err(FlashWriterError::MissErr); }
            if sr.fasterr().bit_is_set() { return Err(FlashWriterError::FastErr); }
        }
    }
    Ok(())
}

#[link_section = ".data"]
#[inline(never)]
fn check_bsy_sram(regs: &mut FLASH) -> Result<(), FlashWriterError> {
    let mut cnt: u16 = 0;
    while regs.sr.read().bsy().bit_is_set() || cnt < 220 { cnt += 1; }
    match regs.sr.read().bsy().bit_is_set() {
        true => { return Err(FlashWriterError::BsyTimeout); }
        false => {
            match check_errors_ram(regs){
                Ok(_) => { Ok(())}
                Err(e) => { Err(e)}
            }
        }
    }
}

#[link_section = ".data"]
#[inline(never)]
fn erase_sram(flash_writer: &mut FlashWriter) -> Result<(), FlashWriterError> {
    for addr in (flash_writer.start_address..=flash_writer.end_address).step_by(PAGE_SIZE) {
        flash_writer.regs.cr.modify(|_, w| w.per().set_bit());
        if USE_PAGE_NUM{
            cfg_if! {
                if #[cfg(feature = "stm32l4x6")]{
                    let mut page_number = ((addr - START_ADDR) / PAGE_SIZE as u32);
                    if page_number > flash_writer.bank_change_on_page_num {
                        flash_writer.regs.cr.modify(|_,w|w.bker().set_bit());
                        page_number = (page_number - flash_writer.bank_change_on_page_num + 1u32) ;
                    }
                    else {
                        flash_writer.regs.cr.modify(|_,w|w.bker().clear_bit());
                    }

                    flash_writer.regs.cr.modify(|_, w| unsafe{ w.pnb().bits(page_number as u8) });
                    flash_writer.regs.cr.modify(|_, w| w.start().set_bit());
                }
            }
        }
        else {
            flash_writer.regs.ar.write(|w| unsafe { w.bits(addr) });
        }
        cfg_if! {
            if #[cfg(feature = "stm32f0xx")] {
                flash_writer.regs.cr.modify(|_, w| w.strt().set_bit());
            }
        }
        match check_bsy_sram(&mut flash_writer.regs) {
            Err(e) => { return Err(e); }
            Ok(_) => {
                flash_writer.regs.cr.modify(|_, w| w.per().clear_bit());
                continue;
            }
        }
    }
    Ok(())
}
#[link_section = ".data"]
#[inline(never)]
fn write_sram(regs: &mut FLASH, address: u32, data: ProgramChunk) -> Result<(), FlashWriterError> {
    let w_a = address as *mut ProgramChunk;
    regs.cr.modify(|_, w| w.pg().set_bit());
    unsafe { core::ptr::write_volatile(w_a, data) };
    match check_bsy_sram(regs) {
        Err(e) => { return Err(e); }
        Ok(_) => { {
            regs.cr.modify(|_, w| w.pg().clear_bit());
            Ok(())
        }
        }
    }
}

impl FlashWriter{
    pub fn new(mut range: RangeInclusive<u32>, regs: FLASH) -> Result<self::FlashWriter, FlashWriterError> {
        let mut flash_range = START_ADDR..=START_ADDR + stm32_device_signature::flash_size_kb() as u32 * 1024u32;
        match check_range(flash_range.borrow_mut(), range.borrow_mut()){
            true => {
                regs.cr.modify(|_,w|w.eopie().set_bit());
                unsafe{ regs.sr.modify(|_,w|w.bits(0x0000_0000)); };
                Ok(
                    FlashWriter{
                        bank_change_on_page_num: (stm32_device_signature::flash_size_kb() as u32 / (PAGE_SIZE * 2 / 1024 ) as u32) - 1u32,
                        start_address: *range.start(),
                        end_address: *range.end(),
                        next_write_address: *range.start(),
                        image_len: 0usize,
                        buffer: WriteBuff{
                            data: [0u8; PROGRAM_SIZE],
                            len: 0
                        },
                        regs
                    })
            }
            false => { Err(FlashWriterError::InvalidRange)}
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

    pub fn get_start_address(&mut self) -> u32 {
        self.start_address
    }

    fn lock(&mut self){
        self.regs.cr.modify(|_,w| w.lock().set_bit());
    }

    pub fn release_regs(self) -> FLASH{
        self.regs
    }

    fn unlock(&mut self) -> Result<(), FlashWriterError>{
        match check_bsy_sram(&mut self.regs){
            Err(e) => { return Err(e); }
            Ok(_) => {
                if self.regs.cr.read().lock().bit_is_set() {
                    self.regs.keyr.write(|w|unsafe{w.bits(KEY_1)});
                    self.regs.keyr.write(|w|unsafe{w.bits(KEY_2)});
                }
                match self.regs.cr.read().lock().bit_is_clear(){
                    true => Ok(()),
                    false => Err(FlashWriterError::FlashLocked),
                }
            }
        }
    }

    pub fn write(&mut self, data: &[u8]) -> Result<(), FlashWriterError> {
        match self.unlock(){
            Err(e) => { return Err(e); }
            Ok(_) => {
                self.image_len += data.len();
                let mut len_to_take = 0usize;
                if self.buffer.len != 0 {
                    len_to_take = PROGRAM_SIZE - self.buffer.len;
                    let mut write_buf= [0xFF; PROGRAM_SIZE];
                    write_buf[0..self.buffer.len].copy_from_slice(&self.buffer.data[0..self.buffer.len]);
                    if data.len() >= len_to_take {
                        write_buf[self.buffer.len..self.buffer.len + len_to_take].copy_from_slice(&data[0..len_to_take]);
                    }
                    else {
                        write_buf[self.buffer.len..self.buffer.len + data.len()].copy_from_slice(&data[0..data.len()]);
                    }
                    self.buffer.len = 0;
                    let mut dat = 0 as ProgramChunk;
                    unsafe {
                        core::ptr::copy_nonoverlapping(write_buf.as_ptr(),
                                                       &mut dat as *mut _ as *mut u8,
                                                       PROGRAM_SIZE)
                    };
                    match write_sram(self.regs.borrow_mut(), self.next_write_address, dat) {
                        Ok(_) => {
                            if self.next_write_address < (self.end_address - PROGRAM_SIZE as u32) {
                                self.next_write_address += PROGRAM_SIZE as u32;
                            }
                            else{
                                return Err(FlashWriterError::OutOfFlashWriterMemory);
                            }
                        }
                        Err(e) => { return Err(e); }
                    }

                }

                if data.len() > len_to_take {
                    let chunks = data[len_to_take..data.len()].chunks_exact(PROGRAM_SIZE);
                    let remainder = chunks.remainder();

                    for bytes in chunks.into_iter() {
                        let mut dat = 0 as ProgramChunk;
                        unsafe {
                            core::ptr::copy_nonoverlapping(bytes.as_ptr(),
                                                           &mut dat as *mut _ as *mut u8,
                                                           PROGRAM_SIZE)
                        };
                        match write_sram(self.regs.borrow_mut(), self.next_write_address, dat) {
                            Ok(_) => {
                                if self.next_write_address < (self.end_address - PROGRAM_SIZE as u32) {
                                    self.next_write_address += PROGRAM_SIZE as u32;
                                }
                                else{
                                    return Err(FlashWriterError::OutOfFlashWriterMemory);
                                }
                            }
                            Err(e) => { return Err(e); }
                        }
                    }
                    self.buffer.data[0..remainder.len()].copy_from_slice(remainder);
                    self.buffer.len = remainder.len();
                }
                Ok(())
            }
        }
    }

    pub fn read<T>(&mut self, addr: u32, len_to_read: usize) -> &[T] {
        unsafe { core::slice::from_raw_parts(addr as *const T, len_to_read) }
    }


    pub fn flush(&mut self) -> Result<(), FlashWriterError> {
        if self.buffer.len != 0 {
            let mut dat = ProgramChunk::max_value();
            for i in 0..self.buffer.len{
                dat = dat << 8 | self.buffer.data[self.buffer.len - 1 - i] as ProgramChunk;
            }
            if self.next_write_address < (self.end_address - PROGRAM_SIZE as u32) {
                match write_sram(self.regs.borrow_mut(), self.next_write_address, dat) {
                    Ok(_) => {
                        self.buffer.len = 0;
                        self.lock();
                        Ok(())
                    }
                    Err(e) => {
                        self.lock();
                        return Err(e);
                    }
                }
            }
            else {
                return Err(FlashWriterError::OutOfFlashWriterMemory);
            }
        }
        else {
            self.lock();
            Ok(())
        }
    }
}

cfg_if!{
 if #[cfg(feature = "stm32f0xx")]{
        type ProgramChunk = u16;
        const USE_PAGE_NUM: bool = false;
        const START_ADDR: u32 = 0x0800_0000;
        const PAGE_SIZE: usize = 1024;
        const PROGRAM_SIZE: usize = 2;
        use stm32f0xx_hal::stm32::FLASH;
    }
}
