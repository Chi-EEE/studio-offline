use iced_x86::{Code, Decoder, DecoderOptions, Mnemonic};
use windows::core::PCSTR;
use windows::Win32::System::LibraryLoader::GetModuleHandleA;
use windows::Win32::System::Memory::*;
use windows::Win32::System::ProcessStatus::{GetModuleInformation, MODULEINFO};
use windows::Win32::System::Threading::GetCurrentProcess;

pub unsafe fn get_module_info(module_name: &str) -> Option<(usize, usize)> {
    let name = std::ffi::CString::new(module_name).ok()?;
    let module_handle = GetModuleHandleA(PCSTR(name.as_ptr() as *const u8)).ok()?;
    let mut module_info = MODULEINFO::default();
    let _ = GetModuleInformation(
        GetCurrentProcess(),
        module_handle,
        &mut module_info,
        std::mem::size_of::<MODULEINFO>() as u32,
    );
    Some((
        module_info.lpBaseOfDll as usize,
        module_info.SizeOfImage as usize,
    ))
}

pub fn aob_scan(pattern_data: (&[u8], &[u8])) -> Option<usize> {
    let (pattern, mask) = pattern_data;
    if pattern.is_empty() || pattern.len() != mask.len() {
        return None;
    }
    unsafe {
        let (module_base, module_size) = get_module_info("RobloxStudioBeta.exe")?;
        let pattern_len = mask.len();

        let mut page_info = MEMORY_BASIC_INFORMATION::default();
        let mut i = module_base;

        while i < module_base + module_size {
            if VirtualQuery(
                Some(i as *const _),
                &mut page_info,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            ) == 0
            {
                i += page_info.RegionSize.max(4096);
                continue;
            }

            if page_info.State != MEM_COMMIT
                || (page_info.Protect.0 & (PAGE_NOACCESS.0 | PAGE_GUARD.0)) != 0
            {
                i += page_info.RegionSize;
                continue;
            }

            let region_end = (i + page_info.RegionSize).min(module_base + module_size);
            for j in 0..(region_end - i).saturating_sub(pattern_len - 1) {
                let mut found = true;
                for k in 0..pattern_len {
                    let char_ptr = (i + j + k) as *const u8;
                    let mask_char = mask[k];
                    let pattern_char = pattern[k];

                    if mask_char != b'?' && pattern_char != *char_ptr {
                        found = false;
                        break;
                    }
                }

                if found {
                    return Some(i + j);
                }
            }
            i += page_info.RegionSize;
        }
    }
    None
}

pub fn scan_string(start: usize, size: usize, target: &str) -> Option<usize> {
    let pattern = target.as_bytes();
    let slice = unsafe { std::slice::from_raw_parts(start as *const u8, size) };

    slice
        .windows(pattern.len())
        .position(|window| window == pattern)
        .map(|offset| start + offset)
}

pub fn scan_xref(start: usize, size: usize, target_addr: usize) -> Option<usize> {
    let end = start + size;
    let mut page_info = MEMORY_BASIC_INFORMATION::default();
    let mut i = start;

    while i < end {
        unsafe {
            if VirtualQuery(
                Some(i as *const _),
                &mut page_info,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            ) == 0
            {
                break;
            }
        }

        if page_info.State == MEM_COMMIT
            && (page_info.Protect
                & (PAGE_EXECUTE
                    | PAGE_EXECUTE_READ
                    | PAGE_EXECUTE_READWRITE
                    | PAGE_EXECUTE_WRITECOPY))
                != PAGE_PROTECTION_FLAGS(0)
        {
            let chunk_size = page_info.RegionSize;
            let chunk_start = i;

            let data = unsafe { std::slice::from_raw_parts(chunk_start as *const u8, chunk_size) };

            for offset in 0..data.len().saturating_sub(7) {
                let b1 = data[offset];
                if (b1 == 0x48 || b1 == 0x4C) && data[offset + 1] == 0x8D {
                    let modrm = data[offset + 2];
                    if (modrm & 0xC7) == 0x05 {
                        let disp = i32::from_le_bytes([
                            data[offset + 3],
                            data[offset + 4],
                            data[offset + 5],
                            data[offset + 6],
                        ]);
                        let next_ip = chunk_start + offset + 7;
                        let target = (next_ip as isize + disp as isize) as usize;

                        if target == target_addr {
                            return Some(chunk_start + offset);
                        }
                    }
                }
            }
        }
        i += page_info.RegionSize;
    }
    None
}

pub fn find_jz_from_cmp_backwards_for_the_security_cookie(xref_addr: usize) -> Option<usize> {
    let max_lookback = 100;
    let start_search = xref_addr.saturating_sub(max_lookback);
    let len = xref_addr - start_search;
    let data = unsafe { std::slice::from_raw_parts(start_search as *const u8, len) };

    for offset in (0..len).rev() {
        let current_start = start_search + offset;
        if xref_addr - current_start < 5 {
            continue;
        }

        let mut decoder = Decoder::with_ip(
            64,
            &data[offset..],
            current_start as u64,
            DecoderOptions::NONE,
        );

        let mut instrs = Vec::new();
        let mut success = false;

        while decoder.can_decode() {
            let instr = decoder.decode();
            if instr.next_ip() as usize == xref_addr {
                success = true;
                instrs.push(instr);
                break;
            }
            if (instr.ip() as usize) > xref_addr {
                break;
            }
            instrs.push(instr);
        }

        if success {
            for i in (0..instrs.len()).rev() {
                let instr = instrs[i];
                match instr.code() {
                    Code::Je_rel8_64 | Code::Je_rel32_64 => {
                        if i > 0 {
                            let prev = instrs[i - 1];
                            if prev.mnemonic() == Mnemonic::Cmp {
                                return Some(instr.ip() as usize);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    None
}
