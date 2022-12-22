/*
Chip8 memory model looks like this: 

+----------------+= 0xFFF (4095) End of Chip-8 RAM
|                |
|                |
|                |
|                |
|                |
| 0x200 to 0xFFF | 
|     Chip-8     |
| Program / Data |
|     Space      |
|                |
|                |
|                |
|                |
|                |
|                |
|                |
+----------------+= 0x200 (512) Start of Chip-8 programs
|                |
| Reserved for   |
|  interpreter   |
+----------------+= 0x000 (0) Start of Chip-8 RAM

Because Chip8 is a Virtual Machine, space reserved for interpreter is the place 
where all the Chip8 internals should be (registers, stack, display memory and other stuff used by specific VM implementation)

Internals required by specification:
- 16 8-bit general purpose registers: V0...VF
- 16-bit index register
- 8-bit Delay Timer
- 8-bit Sound Timer
- 16-bit Program Counter
- 8-bit Stack Pointer
- Stack allowing 16-levels of nested subroutines
- Display buffer for monochromatic 64px x 32px display
- Built-in font sprites

After calculating space required by above internals 
we can calculate how much space we've left for our custom implementation specific Chip8 elements.

256 bytes - Display buffer
 80 bytes - Built-in font
 32 bytes - Stack
 16 bytes - General purpose registers
  2 bytes - Index register
  2 bytes - Program counter
  1 byte  - Delay Timer
  1 byte  - Sound Timer
  1 byte  - Stack Pointer
----------------------------------------
391 bytes - Total

512 - 391 = 121 bytes
 
This implementation will have following memory mapping of the internals:
0x000 - 0x04F : Built-in font
0x050 - 0x05F : V registers
0x060 - 0x07F : Stack
0x080 - 0x17F : Display buffer
0x180         : Stack pointer
0x181         : Sound timer
0x182         : Delay timer
0x183 - 0x184 : Program counter
0x185 - 0x186 : Index register
0x187         : [CUSTOM] Wait key press flag (If most significant bit is indicating if VM is waiting for key, 4 least significant bits tells to which register save the key)
0x188 - 0x198 : [CUSTOM] Key presses flags
*/

macro_rules! decode_instruction {
    ($mnemonic:literal, $condition:expr, $op:expr) => {
        if $condition {
            $op;
        }
    }
}

const FONT: [u32; 16] = [
    0xF999F, 0x26227, 0xF1F8F, 0xF1F1F,
    0x99F11, 0xF8F1F, 0xF8F9F, 0xF1244,
    0xF9F9F, 0xF9F1F, 0xF9F99, 0xE9E9E,
    0xF888F, 0xE999E, 0xF8F8F, 0xF8F88
];

const FONT_ADDR: u16                = 0x0000;
const V_REGISTERS_ADDR: u16         = 0x0050;
const STACK_ADDR: u16               = 0x0060;
const DISPLAY_BUFFER_ADDR: u16      = 0x0080;
const STACK_POINTER_ADDR: u16       = 0x0180;
const SOUND_TIMER_ADDR: u16         = 0x0181;
const DELAY_TIMER_ADDR: u16         = 0x0182;
const PROGRAM_COUNTER_ADDR: u16     = 0x0183;
const INDEX_REGISTER_ADDR: u16      = 0x0185;
const WAIT_KEY_PRESS_FLAG_ADDR: u16 = 0x0187;
const KEY_FLAGS_ADDR: u16           = 0x0188;

const DISPLAY_WIDTH: usize = 64;
const DISPLAY_HEIGHT: usize = 32;
const DISPLAY_BUFFER_SIZE: usize = DISPLAY_HEIGHT * DISPLAY_WIDTH / 8;

pub struct Chip8 {
    memory: [u8; 0x1000]
}

impl Chip8 {
    pub fn new() -> Self {
        let mut chip8 = Self{ memory: [0; 0x1000] };

        // Install font
        for digit in 0..16 {
            for byte in 0..5 {
                chip8.memory[digit * byte] = (FONT[digit] >> (16 - byte * 4) & 0xFF) as u8;
            }
        }

        return chip8;
    }

    pub fn get_display_buffer(&self) -> [u8; DISPLAY_BUFFER_SIZE] {
        let start = usize::from(DISPLAY_BUFFER_ADDR);
        let end: usize = usize::from(DISPLAY_BUFFER_ADDR) + usize::from(DISPLAY_BUFFER_SIZE);
        return self.memory[start..end]
            .try_into()
            .expect("Slice containing display buffer has incorrect length");
    }

    pub fn execute_instruction(&mut self) {
        if self.is_waiting_for_key() {
            return;
        }
        
        let opcode = self.fetch_opcode();

        let c = opcode >> 12 & 0xF;
        let nnn: u16 = opcode & 0xFFF;
        let nn: u8 = (opcode & 0xFF) as u8;
        let n: u8 = (opcode & 0xF) as u8;
        let x: u8 = (opcode >> 8 & 0xF) as u8;
        let y: u8 = (opcode >> 4 & 0xF) as u8;

        decode_instruction!("CLS"          , c == 0x0 && nn == 0xE0, self.cls());
        decode_instruction!("RET"          , c == 0x0 && nn == 0xEE, self.ret());
        decode_instruction!("JMP nnn"      , c == 0x1              , self.jmp_direct(nnn));
        decode_instruction!("CALL nnn"     , c == 0x2              , self.call(nnn));
        decode_instruction!("SE Vx, nn"    , c == 0x3              , self.se_immedate(x, nn));
        decode_instruction!("SNE Vx, nn"   , c == 0x4              , self.sne_immedate(x, nn));
        decode_instruction!("SE Vx, Vy"    , c == 0x5 && n == 0x0  , self.se_registers(x, y));
        decode_instruction!("LD Vx, nn"    , c == 0x6              , self.ld_immedate(x, nn));
        decode_instruction!("ADD Vx, nn"   , c == 0x7              , self.add_immedate(x, nn));
        decode_instruction!("LD Vx, Vy"    , c == 0x8 && n == 0x0  , self.ld_registers(x, y));
        decode_instruction!("OR Vx, Vy"    , c == 0x8 && n == 0x1  , self.or_registers(x, y));
        decode_instruction!("AND Vx, Vy"   , c == 0x8 && n == 0x2  , self.and_registers(x, y));
        decode_instruction!("XOR Vx, Vy"   , c == 0x8 && n == 0x3  , self.xor_registers(x, y));
        decode_instruction!("ADD Vx, Vy"   , c == 0x8 && n == 0x4  , self.add_registers(x, y));
        decode_instruction!("SUB Vx, Vy"   , c == 0x8 && n == 0x5  , self.sub_registers(x, y));
        decode_instruction!("SHR Vx"       , c == 0x8 && n == 0x6  , self.shr(x));
        decode_instruction!("SUBN Vx, Vy"  , c == 0x8 && n == 0x7  , self.subn_registers(x, y));
        decode_instruction!("SHL Vx"       , c == 0x8 && n == 0xE  , self.shl(x));
        decode_instruction!("SNE Vx, Vy"   , c == 0x9 && n == 0x0  , self.sne_registers(x, y));
        decode_instruction!("LD I, nnn"    , c == 0xA              , self.ld_index(nnn));
        decode_instruction!("JMP V0, nnn"  , c == 0xB              , self.jmp_indirect(nnn));
        decode_instruction!("RND Vx, nn"   , c == 0xC              , self.rnd(x, nn));
        decode_instruction!("DRW Vx, Vy, n", c == 0xD              , self.drw(x, y, n));
        decode_instruction!("SKP Vx"       , c == 0xE && nn == 0x9E, self.skp(x));
        decode_instruction!("SKNP Vx"      , c == 0xE && nn == 0xA1, self.sknp(x));
        decode_instruction!("LD Vx, DT"    , c == 0xF && nn == 0x07, self.ld_from_dt(x));
        decode_instruction!("LD Vx, K"     , c == 0xF && nn == 0x0A, self.ld_key_press(x));
        decode_instruction!("LD DT, Vx"    , c == 0xF && nn == 0x15, self.ld_into_dt(x));
        decode_instruction!("LD ST, Vx"    , c == 0xF && nn == 0x18, self.ld_into_st(x));
        decode_instruction!("ADD I, Vx"    , c == 0xF && nn == 0x1E, self.add_index(x));
        decode_instruction!("LD F, Vx"     , c == 0xF && nn == 0x29, self.ld_font_addr(x));
        decode_instruction!("LD B, Vx"     , c == 0xF && nn == 0x33, self.ld_bcd(x));
        decode_instruction!("LD [I], Vx"   , c == 0xF && nn == 0x55, self.ld_into_mem(x));
        decode_instruction!("LD Vx, [I]"   , c == 0xF && nn == 0x65, self.ld_from_mem(x));

        self.set_pc(self.get_pc() + 2);
    }

    pub fn get_key_pressed(&self, key: u8) -> bool {
        return self.memory[(KEY_FLAGS_ADDR + u16::from(key)) as usize] > 0
    }

    pub fn set_key_pressed(&mut self, key: u8, pressed: bool) {
        self.memory[(KEY_FLAGS_ADDR + u16::from(key)) as usize] = if pressed { 1 } else { 0 };
        if self.is_waiting_for_key() && pressed {
            self.set_awaited_key(key);
            self.clear_waiting_for_key();
        }
    }

    pub fn is_waiting_for_key(&self) -> bool {
        return self.memory[WAIT_KEY_PRESS_FLAG_ADDR as usize] & 0x80 > 0;
    }

    fn set_waiting_for_key(&mut self) {
        self.memory[WAIT_KEY_PRESS_FLAG_ADDR as usize] |= 1 << 7;
    }

    fn clear_waiting_for_key(&mut self) {
        self.memory[WAIT_KEY_PRESS_FLAG_ADDR as usize] &= !(1 << 7);
    }

    fn get_waiting_key_destination(&self) -> u8 {
        return self.memory[WAIT_KEY_PRESS_FLAG_ADDR as usize] & 0xF;
    }

    fn set_waiting_key_destination(&mut self, x: u8) {
        self.memory[WAIT_KEY_PRESS_FLAG_ADDR as usize] &= 0xF0;
        self.memory[WAIT_KEY_PRESS_FLAG_ADDR as usize] |= x;
    }

    fn set_awaited_key(&mut self, key: u8) {
        let x = self.get_waiting_key_destination();
        self.set_v(x, key);
    }

    fn get_v(&self, index: u8) -> u8 {
        return self.memory[(V_REGISTERS_ADDR + u16::from(index)) as usize];
    }

    fn set_v(&mut self, index: u8, value: u8) {
        self.memory[(V_REGISTERS_ADDR + u16::from(index)) as usize] = value;
    }
    
    fn get_dt(&self) -> u8 {
        return self.memory[DELAY_TIMER_ADDR as usize];
    }

    fn set_dt(&mut self, value: u8) {
        self.memory[DELAY_TIMER_ADDR as usize] = value;
    }
    
    fn get_st(&self) -> u8 {
        return self.memory[SOUND_TIMER_ADDR as usize];
    }

    fn set_st(&mut self, value: u8) {
        self.memory[SOUND_TIMER_ADDR as usize] = value;
    }
    
    fn get_i(&self) -> u16 {
        return u16::from(self.memory[INDEX_REGISTER_ADDR as usize]) << 8 
            | u16::from(self.memory[(INDEX_REGISTER_ADDR + 1) as usize]);
    }

    fn set_i(&mut self, value: u16) {
        self.memory[INDEX_REGISTER_ADDR as usize] = (value >> 8 & 0xFF) as u8;
        self.memory[(INDEX_REGISTER_ADDR + 1) as usize] = (value & 0xFF) as u8; 
    }
    
    fn get_sp(&self) -> u8 {
        return self.memory[STACK_POINTER_ADDR as usize];
    }

    fn set_sp(&mut self, value: u8) {
        self.memory[STACK_POINTER_ADDR as usize] = value;
    }

    fn get_pc(&self) -> u16 {
        return u16::from(self.memory[PROGRAM_COUNTER_ADDR as usize]) << 8 
            | u16::from(self.memory[(PROGRAM_COUNTER_ADDR + 1) as usize])
    }

    fn set_pc(&mut self, value: u16) {
        self.memory[PROGRAM_COUNTER_ADDR as usize] = (value >> 8 & 0xFF) as u8;
        self.memory[(PROGRAM_COUNTER_ADDR + 1) as usize] = (value & 0xFF) as u8;
    }

    fn pop_from_stack(&mut self) -> u16 {
        let offset = self.get_sp() * 2;
        self.set_sp(self.get_sp() - 1);
        return u16::from(self.memory[(STACK_ADDR + u16::from(offset)) as usize]) >> 8 
            | u16::from(self.memory[(STACK_ADDR + u16::from(offset) + 1) as usize])
    }

    fn push_into_stack(&mut self, value: u16) {
        self.set_sp(self.get_sp() + 1);
        let offset = self.get_sp() * 2;
        self.memory[(STACK_ADDR + u16::from(offset)) as usize] = (value >> 8 & 0xFF) as u8;
        self.memory[(STACK_ADDR + u16::from(offset) + 1) as usize] = (value & 0xFF) as u8;
    }

    fn fetch_opcode(&self) -> u16 {
        let pc = self.get_pc();
        return u16::from(self.memory[pc as usize]) >> 8 
            | u16::from(self.memory[(pc + 1) as usize]);
    }

    fn cls(&mut self) {
        for index in 0..DISPLAY_BUFFER_SIZE {
            self.memory[DISPLAY_BUFFER_ADDR as usize + index] = 0;
        }
    }

    fn ret(&mut self) {
        let addr = self.pop_from_stack();
        self.set_pc(addr);
    }

    fn jmp_direct(&mut self, addr: u16) {
        self.set_pc(addr);
    }

    fn call(&mut self, addr: u16) {
        let current_pc = self.get_pc();
        self.push_into_stack(current_pc);
        self.set_pc(addr);
    }

    fn se_immedate(&mut self, x: u8, byte: u8) {
        let vx = self.get_v(x);
        if vx == byte {
            self.set_pc(self.get_pc() + 2);
        }
    }

    fn sne_immedate(&mut self, x: u8, byte: u8) {
        let vx = self.get_v(x);
        if vx != byte {
            self.set_pc(self.get_pc() + 2);
        }
    }

    fn se_registers(&mut self, x: u8, y: u8) {
        let vx = self.get_v(x);
        let vy = self.get_v(y);
        if vx == vy {
            self.set_pc(self.get_pc() + 2);
        }
    }

    fn sne_registers(&mut self, x: u8, y: u8) {
        let vx = self.get_v(x);
        let vy = self.get_v(y);
        if vx != vy {
            self.set_pc(self.get_pc() + 2);
        }
    }

    fn ld_immedate(&mut self, x: u8, byte: u8) {
        self.set_v(x, byte);
    }

    fn add_immedate(&mut self, x: u8, byte: u8) {
        let vx = self.get_v(x);
        self.set_v(x, vx + byte);
    }

    fn ld_registers(&mut self, x: u8, y: u8) {
        let vy = self.get_v(y);
        self.set_v(x, vy);
    }

    fn or_registers(&mut self, x: u8, y: u8) {
        let vx = self.get_v(x);
        let vy = self.get_v(y);
        self.set_v(x, vx | vy);
    }

    fn and_registers(&mut self, x: u8, y: u8) {
        let vx = self.get_v(x);
        let vy = self.get_v(y);
        self.set_v(x, vx & vy);
    }

    fn xor_registers(&mut self, x: u8, y: u8) {
        let vx = self.get_v(x);
        let vy = self.get_v(y);
        self.set_v(x, vx ^ vy);
    }

    fn add_registers(&mut self, x: u8, y: u8) {
        let vx = self.get_v(x);
        let vy = self.get_v(y);
        self.set_v(0xF, if u16::from(vx) + u16::from(vy) > 0xFF { 1 } else { 0 });
        self.set_v(x, vx + vy);
    }

    fn sub_registers(&mut self, x: u8, y: u8) {
        let vx = self.get_v(x);
        let vy = self.get_v(y);
        self.set_v(0xF, if vx > vy { 1 } else { 0 });
        self.set_v(x, vx - vy);
    }

    fn subn_registers(&mut self, x: u8, y: u8) {
        let vx = self.get_v(x);
        let vy = self.get_v(y);
        self.set_v(0xF, if vy > vx { 1 } else { 0 });
        self.set_v(x, vy - vx);
    }

    fn shr(&mut self, x: u8) {
        let vx = self.get_v(x);
        self.set_v(0xF, vx & 0x01);
        self.set_v(x, vx >> 1);
    }

    fn shl(&mut self, x: u8) {
        let vx = self.get_v(x);
        self.set_v(0xF, vx >> 7);
        self.set_v(x, vx << 1);
    }

    fn ld_index(&mut self, addr: u16) {
        self.set_i(addr);
    }

    fn jmp_indirect(&mut self, addr: u16) {
        let v0 = self.get_v(0);
        self.set_pc(addr + u16::from(v0));
    }

    fn rnd(&mut self, x: u8, byte: u8) {
        self.set_v(x, rand::random::<u8>() & byte);
    }

    fn drw(&mut self, x: u8, y: u8, n: u8) {
        let put = |addr: u16, data: u8| {
            self.memory[usize::from(DISPLAY_BUFFER_ADDR) + usize::from(addr)] ^= data;
            return (self.memory[usize::from(DISPLAY_BUFFER_ADDR) + usize::from(addr)] ^ data) & data;
        };

        let mut overflow = 0;
        let vx = self.get_v(x);
        let vy = self.get_v(y);
        let index = self.get_i();
        const WIDTH: u8 = DISPLAY_WIDTH as u8;
        const HEIGHT: u8 = DISPLAY_HEIGHT as u8;
        
        for i in 0..n {
            let addr_left = u16::from(vx % WIDTH + (vy + i) % HEIGHT * WIDTH / 8);
            let addr_right = u16::from((vx + 7) % WIDTH + (vy + i) % HEIGHT * WIDTH / 8);

            let data_left = self.memory[usize::from(index) + usize::from(i)] >> vx % 8;
            let data_right = self.memory[usize::from(index) + usize::from(i)] << 8 - vx % 8;

            self.memory[usize::from(DISPLAY_BUFFER_ADDR) + usize::from(addr_left)] ^= data_left;
            overflow |= (self.memory[usize::from(DISPLAY_BUFFER_ADDR) + usize::from(addr_left)] ^ data_left) & data_left;

            self.memory[usize::from(DISPLAY_BUFFER_ADDR) + usize::from(addr_right)] ^= data_right;
            overflow |= (self.memory[usize::from(DISPLAY_BUFFER_ADDR) + usize::from(addr_right)] ^ data_right) & data_right;
        }

        self.set_v(0xF, if overflow != 0 { 1 } else { 0 });
    }

    fn skp(&mut self, x: u8) {
        let vx = self.get_v(x);
        if self.get_key_pressed(vx) {
            self.set_pc(self.get_pc() + 2);
        }
    }

    fn sknp(&mut self, x: u8) {
        let vx = self.get_v(x);
        if !self.get_key_pressed(vx) {
            self.set_pc(self.get_pc() + 2);
        }
    }

    fn ld_from_dt(&mut self, x: u8) {
        self.set_v(x, self.get_dt());
    }

    fn ld_key_press(&mut self, x: u8) {
        self.set_waiting_key_destination(x);
        self.set_waiting_for_key();
    }

    fn ld_into_dt(&mut self, x: u8) {
        let vx = self.get_v(x);
        self.set_dt(vx);
    }

    fn ld_into_st(&mut self, x: u8) {
        let vx = self.get_v(x);
        self.set_st(vx);
    }

    fn add_index(&mut self, x: u8) {
        let vx = self.get_v(x);
        let i = self.get_i();
        self.set_i(i + u16::from(vx));
    }

    fn ld_font_addr(&mut self, x: u8) {
        let vx = self.get_v(x);
        self.set_i(FONT_ADDR + u16::from(vx) * 5);
    }

    fn ld_bcd(&mut self, x: u8) {
        let vx = self.get_v(x);
        let i = self.get_i();
        self.memory[i as usize] = vx / 100 % 10;
        self.memory[i as usize + 1] = vx / 10 % 10;
        self.memory[i as usize + 2] = vx % 10;
    }

    fn ld_into_mem(&mut self, x: u8) {
        let i = self.get_i();
        for v_reg_index in 0..=x {
            self.memory[(i + u16::from(v_reg_index)) as usize] = self.get_v(v_reg_index);
        }
    }

    fn ld_from_mem(&mut self, x: u8) {
        let i = self.get_i();
        for v_reg_index in 0..=x {
            self.set_v(v_reg_index, self.memory[(i + u16::from(v_reg_index)) as usize])
        }
    }
}