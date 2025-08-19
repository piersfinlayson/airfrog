#!/usr/bin/env python3

# STM32F4 Performance checking script
#
# Reads various STM32F4 registers while the MCU is running whatever workload
# is in firmware.

import requests
import time
import json
import argparse
import sys

# Base URL will be set from command line argument
BASE_URL = None

# Assumed clock frequencies for HSI and HSE
HSI_FREQ = 16_000_000
HSE_FREQ = 8_000_000

#
# Register addresses grouped by peripheral
#

# DWT and CoreDebug registers.
#
# These are ARMv7 specific registers, documented in ARM DDI0403
DEMCR_ADDR = 0xE000EDFC
DWT_CTRL_ADDR = 0xE0001000
DWT_CYCCNT_ADDR = 0xE0001004
DWT_CPICNT_ADDR = 0xE0001008
DWT_EXCCNT_ADDR = 0xE000100C
DWT_SLEEPCNT_ADDR = 0xE0001010
DWT_LSUCNT_ADDR = 0xE0001014
DWT_FOLDCNT_ADDR = 0xE0001018

# Cortex-M4 System Control Block
#
# Documented in ARM DDI0403
SCB_STCSR_ADDR = 0xE000E010
SCB_ICSR_ADDR = 0xE000ED04

NVIC_IABR0_ADDR = 0xE000E300
NUM_NVIC_IABR_REG = 16

DBGMCU_IDCODE = 0xE0042000

# STM32F4 Flash registers
#
# Documented in the STM32F4* Reference Manuals, e.g. RM0090 for STM32F405
# There is one minor difference between models - F405/415/407/417 use bits 0-2
# for flash wait states.  The remainder is bits 0-3 (allowing more wait states
# to be specified).
FLASH_BASE = 0x40023C00
FLASH_ACR_ADDR = FLASH_BASE + 0x00

# STM32F4 RCC registers
#
# Documented in the STM32F4* Reference manuals.
#
# PWR_EN, bit 28 of RCC_APB1ENR must be set to enable higher clock speeds on
# the F405.
RCC_BASE = 0x40023800
RCC_CR_ADDR = RCC_BASE + 0x00
RCC_PLLCFGR_ADDR = RCC_BASE + 0x04
RCC_CFGR_ADDR = RCC_BASE + 0x08
RCC_CIR_ADDR = RCC_BASE + 0x0C
RCC_AHB1ENR_ADDR = RCC_BASE + 0x30
RCC_APB1ENR_ADDR = RCC_BASE + 0x40

# STM32F4 Power registers
#
# Bit 14 (F405/415/407/417) sets VOS scale mode 1, for higher clock speeds.
# Bits 14-15 (F411 > 84 MHz) sets VOS scale mode 1, for > 84 MHz clock speeds.
PWR_BASE = 0x40007000
PWR_CR_ADDR = PWR_BASE + 0x00
PWR_CSR_ADDR = PWR_BASE + 0x04

# STM32F4 GPIO registers
GPIO_BASE = 0x40020000
GPIO_SIZE = 0x400
OSPEEDR_OFFSET = 0x08
PUPDR_OFFSET = 0x0C
MODER_OFFSET = 0x00
OTYPER_OFFSET = 0x04

def write_memory(addr, value):
    """Write 32-bit value to memory address via airfrog API"""
    url = f"{BASE_URL}/target/memory/write/0x{addr:X}"
    response = requests.post(url, json={"data": f"0x{value:08X}"})
    if response.status_code != 200:
        raise Exception(f"Write failed: {response.text}")

def read_memory(addr):
    """Read 32-bit value from memory address via airfrog API"""
    url = f"{BASE_URL}/target/memory/read/0x{addr:X}"
    response = requests.get(url)
    if response.status_code != 200:
        raise Exception(f"Read failed: {response.text}")
    return int(response.json()["data"], 16)

def reset_target():
    """Reset and initialize target for debugging"""
    print("Resetting and initializing target...")
    url = f"{BASE_URL}/target/reset"
    response = requests.post(url, json={})
    if response.status_code != 200:
        raise Exception(f"Target reset failed: {response.text}")
    
    # Show target info from reset response
    result = response.json()
    if "status" in result:
        status = result["status"]
        print(f"  Target connected: {status.get('connected', 'unknown')}")
        print(f"  MCU: {status.get('mcu', 'unknown')}")
        print(f"  IDCODE: {status.get('idcode', 'unknown')}")
    print("Target reset complete!\n")
    
def read_gpio_speed(port):
    """Read GPIO port speed from OSPEEDR register"""
    if port not in 'ABCDEFGH':
        raise ValueError("Invalid GPIO port. Must be A-H.")
    
    offset = (ord(port) - ord('A')) * GPIO_SIZE + GPIO_BASE
    ospeedr_addr = offset + OSPEEDR_OFFSET
    ospeedr = read_memory(ospeedr_addr)
    
    speeds = []
    for i in range(16):
        speed = (ospeedr >> (i * 2)) & 3
        if speed == 0:
            speeds.append("L")
        elif speed == 1:
            speeds.append("M")
        elif speed == 2:
            speeds.append("H")
        elif speed == 3:
            speeds.append("VH")
    
    print(f"    GPIO{port} OSPEEDR: 0x{ospeedr:08X}")
    print(f"      Speed settings 0-15: {','.join(speeds)}")

def read_gpio_pupd(port):
    """Read GPIO port pull-up/pull-down from PUPDR register"""
    if port not in 'ABCDEFGH':
        raise ValueError("Invalid GPIO port. Must be A-H.")
    
    offset = (ord(port) - ord('A')) * GPIO_SIZE + GPIO_BASE
    pupdr_addr = offset + PUPDR_OFFSET
    pupdr = read_memory(pupdr_addr)
    
    pull_settings = []
    for i in range(16):
        pupd = (pupdr >> (i * 2)) & 3
        if pupd == 0:
            pull_settings.append("None")
        elif pupd == 1:
            pull_settings.append("PU")
        elif pupd == 2:
            pull_settings.append("PD")
        elif pupd == 3:
            pull_settings.append("Rsvd")
    
    print(f"    GPIO{port} PUPDR: 0x{pupdr:08X}")
    print(f"      Pull settings 0-15: {','.join(pull_settings)}")

def read_gpio_mode(port):
    """Read GPIO port mode from MODER register"""
    if port not in 'ABCDEFGH':
        raise ValueError("Invalid GPIO port. Must be A-H.")
    
    offset = (ord(port) - ord('A')) * GPIO_SIZE + GPIO_BASE
    moder_addr = offset + MODER_OFFSET
    moder = read_memory(moder_addr)
    
    modes = []
    for i in range(16):
        mode = (moder >> (i * 2)) & 3
        if mode == 0:
            modes.append("In")
        elif mode == 1:
            modes.append("Out")
        elif mode == 2:
            modes.append("AF")
        elif mode == 3:
            modes.append("Alg")
    
    print(f"    GPIO{port} MODER: 0x{moder:08X}")
    print(f"      Mode settings 0-15: {','.join(modes)}")
    
def read_gpio_otype(port):
    """Read GPIO port output type from OTYPER register"""
    if port not in 'ABCDEFGH':
        raise ValueError("Invalid GPIO port. Must be A-H.")
    
    offset = (ord(port) - ord('A')) * GPIO_SIZE + GPIO_BASE
    otyper_addr = offset + OTYPER_OFFSET
    otyper = read_memory(otyper_addr)
    
    types = []
    for i in range(16):
        otype = (otyper >> i) & 1
        types.append("PP" if otype == 0 else "OD")
    
    print(f"    GPIO{port} OTYPER: 0x{otyper:08X}")
    print(f"      Output types 0-15: {','.join(types)}")

def read_flash_acr():
    """Read and display FLASH_ACR (Flash Access Control Register) at 0x40023C00
    
    Bit 10: DCEN (Data cache enable) - F405/411 don't have data cache, should read 0
    Bit 9:  ICEN (Instruction cache enable)
    Bit 8:  PRFTEN (Prefetch enable)
    Bits 2:0: LATENCY (Flash wait states)
    """
    flash_acr = read_memory(FLASH_ACR_ADDR)
    dcen = (flash_acr >> 10) & 1
    icen = (flash_acr >> 9) & 1
    prften = (flash_acr >> 8) & 1
    # On the F405 only bits 2:0 cover LATENCY (flash wait states), but bit 3
    # is reserved so this should be OK 
    latency = flash_acr & 0xF
    print(f"  FLASH_ACR: 0x{flash_acr:08X}")
    print(f"    DCEN (bit 10): {dcen} (data cache enabled)")
    print(f"    ICEN (bit 9):  {icen} (instruction cache enabled)")
    print(f"    PRFTEN (bit 8): {prften} (prefetch enabled)")
    print(f"    LATENCY (bits 3:0): {latency} (wait states)")

def read_rcc_cr():
    """Read and display RCC_CR (RCC Clock Control Register) at 0x40023800
    
    Bit 25: PLLRDY (PLL ready)
    Bit 24: PLLON (PLL enable)
    Bit 17: HSERDY (HSE ready)
    Bit 16: HSEON (HSE enable)
    Bit 1:  HSIRDY (HSI ready)
    Bit 0:  HSION (HSI enable)
    """
    rcc_cr = read_memory(RCC_CR_ADDR)
    pllrdy = (rcc_cr >> 25) & 1
    pllon = (rcc_cr >> 24) & 1
    hserdy = (rcc_cr >> 17) & 1
    hseon = (rcc_cr >> 16) & 1
    hsirdy = (rcc_cr >> 1) & 1
    hsion = rcc_cr & 1
    print(f"  RCC_CR: 0x{rcc_cr:08X}")
    print(f"    PLL: ON={pllon} RDY={pllrdy}")
    print(f"    HSE: ON={hseon} RDY={hserdy}")
    print(f"    HSI: ON={hsion} RDY={hsirdy}")

def read_rcc_pllcfgr():
    """Read and display RCC_PLLCFGR (PLL Configuration Register) at 0x40023804
    
    Bits 27:24: PLLQ (division factor for USB/SDIO clocks)
    Bit 22:     PLLSRC (PLL source: 0=HSI, 1=HSE)
    Bits 17:16: PLLP (division factor for main system clock)
    Bits 14:6:  PLLN (multiplication factor)
    Bits 5:0:   PLLM (division factor for input clock)
    """
    rcc_pllcfgr = read_memory(RCC_PLLCFGR_ADDR)
    pllq = (rcc_pllcfgr >> 24) & 0xF
    pllsrc = (rcc_pllcfgr >> 22) & 1
    pllp = ((rcc_pllcfgr >> 16) & 3) * 2 + 2  # 00=2, 01=4, 10=6, 11=8
    plln = (rcc_pllcfgr >> 6) & 0x1FF
    pllm = rcc_pllcfgr & 0x3F
    
    input_freq = HSE_FREQ if pllsrc else HSI_FREQ
    vco_freq = (input_freq // pllm) * plln
    sysclk_freq = vco_freq // pllp
    usb_freq = vco_freq // pllq
    
    
    print(f"  RCC_PLLCFGR: 0x{rcc_pllcfgr:08X}")
    print(f"    PLLQ={pllq} PLLSRC={'HSE' if pllsrc else 'HSI'} PLLP={pllp} PLLN={plln} PLLM={pllm}")
    print(f"      SYSCLK: {sysclk_freq/1_000_000:.1f} MHz")
    print(f"        Input: {input_freq/1_000_000:.1f} MHz ({'HSE' if pllsrc else 'HSI'})")
    print(f"        VCO: {vco_freq/1_000_000:.1f} MHz")
    print(f"        USB: {usb_freq/1_000_000:.1f} MHz")

def get_pre_str(ppre):
    """Get prescaler string for given PPRE value"""
    if ppre == 0b100:
        return "/2"
    elif ppre == 0b101:
        return "/4"
    elif ppre == 0b110:
        return "/8"
    elif ppre == 0b111:
        return "/16"
    else:
        return "/1"

def get_hpre_str(hpre):
    """Get AHB prescaler string for given HPRE value"""
    if hpre == 0b1000:
        return "/2"
    elif hpre == 0b1001:
        return "/4"
    elif hpre == 0b1010:
        return "/8"
    elif hpre == 0b1011:
        return "/16"
    elif hpre == 0b1100:
        return "/64"
    elif hpre == 0b1101:
        return "/128"
    elif hpre == 0b1110:
        return "/256"
    elif hpre == 0b1111:
        return "/512"
    else:
        return "/1"

def read_rcc_cfgr():
    """Read and display RCC_CFGR (Clock Configuration Register) at 0x40023808
    
    Bits 15:13: PPRE2 (APB2 prescaler)
    Bits 12:10: PPRE1 (APB1 prescaler) 
    Bits 7:4:   HPRE (AHB prescaler)
    Bits 3:2:   SWS (System clock switch status)
    Bits 1:0:   SW (System clock switch)
    """
    rcc_cfgr = read_memory(RCC_CFGR_ADDR)
    ppre2 = (rcc_cfgr >> 13) & 7
    ppre2_str = get_pre_str(ppre2)
    ppre1 = (rcc_cfgr >> 10) & 7
    ppre1_str = get_pre_str(ppre1)
    hpre = (rcc_cfgr >> 4) & 0xF
    hpre_str = get_hpre_str(hpre)
    sws = (rcc_cfgr >> 2) & 3
    sw = rcc_cfgr & 3
    clock_sources = ["HSI", "HSE", "PLL", "Reserved"]
    print(f"  RCC_CFGR: 0x{rcc_cfgr:08X}")
    print(f"    System clock: SW={clock_sources[sw]} SWS={clock_sources[sws]}")
    print(f"    Prescalers:")
    print(f"      AHB={hpre_str} ({hpre})")
    print(f"      APB1={ppre1_str} ({ppre1})")
    print(f"      APB2={ppre1_str} ({ppre2})")

def read_rcc_cir():
    """Read and display RCC_CIR (Clock Interrupt Register) at 0x4002380C
    """
    rcc_cir = read_memory(RCC_CIR_ADDR)
    print(f"  RCC_CIR: 0x{rcc_cir:08X}")

def read_rcc_ahb1enr():
    """Read and display GPIO port enables from RCC_AHB1ENR at 0x40023830
    
    Bit 8: GPIOIEN (GPIO port I clock enable) - F405/415/407/417 only
    Bit 7: GPIOHEN (GPIO port H clock enable)
    Bit 6: GPIOGEN (GPIO port G clock enable) - F405/415/407/417 only
    Bit 5: GPIOFEN (GPIO port F clock enable)
    Bit 4: GPIOEEN (GPIO port E clock enable)
    Bit 3: GPIODEN (GPIO port D clock enable)
    Bit 2: GPIOCEN (GPIO port C clock enable)
    Bit 1: GPIOBEN (GPIO port B clock enable)
    Bit 0: GPIOAEN (GPIO port A clock enable)
    """
    rcc_ahb1enr = read_memory(RCC_AHB1ENR_ADDR)

    print(f"  RCC_AHB1ENR: 0x{rcc_ahb1enr:08X}")
    
    gpio_ports = ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I']
    enabled_ports = []
    
    for i, port in enumerate(gpio_ports):
        if i < 9 and (rcc_ahb1enr >> i) & 1:
            enabled_ports.append(port)
    
    print(f"    GPIO enabled ports: {', '.join(enabled_ports) if enabled_ports else 'None'}")

def read_rcc_apb1enr():
    """Read and display RCC_APB1ENR (APB1 Peripheral Clock Enable Register) at 0x40023840
    
    Bit 28: PWREN (Power interface clock enable)
    Plus other APB1 peripheral enables
    """
    rcc_apb1enr = read_memory(RCC_APB1ENR_ADDR)
    pwren = (rcc_apb1enr >> 28) & 1
    print(f"  RCC_APB1ENR: 0x{rcc_apb1enr:08X}")
    print(f"    PWREN (bit 28): {pwren} (power interface clock)")

def read_pwr_cr():
    """Read and display PWR_CR (Power Control Register) at 0x40007000
    
    Bits 15:14: VOS (Voltage scaling selection)
    Bit 9:      FPDS (Flash power down in Stop mode)
    Bit 8:      DBP (Disable backup domain write protection)
    Bits 7:5:   PLS (PVD level selection)
    Bit 4:      PVDE (Power voltage detector enable)
    Bit 3:      CSBF (Clear standby flag)
    Bit 2:      CWUF (Clear wakeup flag)
    Bit 1:      PDDS (Power down deepsleep)
    Bit 0:      LPDS (Low-power deepsleep)
    """
    pwr_cr = read_memory(PWR_CR_ADDR)
    vos = (pwr_cr >> 14) & 3
    fpds = (pwr_cr >> 9) & 1
    dbp = (pwr_cr >> 8) & 1
    pvde = (pwr_cr >> 4) & 1
    print(f"  PWR_CR: 0x{pwr_cr:08X}")
    print(f"    VOS (bits 15:14): {vos} (voltage scaling)")

def read_pwr_csr():
    """
    Read and display PWR_CSR (Power Control Status Register) at 0x40007004

    Bit 14:     VOS Rdy
    """
    pwr_csr = read_memory(PWR_CSR_ADDR)
    vos_rdy = (pwr_csr >> 14) & 1
    print(f"  PWR_CSR: 0x{pwr_csr:08X}")
    print(f"    VOS Rdy (bit 14): {vos_rdy}")

def read_scb_stcsr():
    """Read and display SCB_STCSR (SysTick Control and Status Register) at 0xE000E010
    """
    scb_stcsr = read_memory(SCB_STCSR_ADDR)
    print(f"  SCB_STCSR: 0x{scb_stcsr:08X}")
    tickint = (scb_stcsr >> 1) & 1
    enable = scb_stcsr & 1
    print(f"    SysTick: ENABLE={enable} TICKINT={tickint}")

def read_scb_icsr():
    """Read and display SCB_ICSR (Interrupt Control and State Register) at 0xE000ED04
    
    Bit 31:     NMIPENDSET (NMI set-pending bit)
    Bit 28:     PENDSVSET (PendSV set-pending bit)
    Bit 27:     PENDSVCLR (PendSV clear-pending bit)
    Bit 26:     PENDSTSET (SysTick set-pending bit)
    Bit 25:     PENDSTCLR (SysTick clear-pending bit)
    Bit 23:     ISRPREEMPT (Interrupt preempted active)
    Bit 22:     ISRPENDING (Interrupt pending flag)
    Bits 20:12: VECTPENDING (Pending vector number)
    Bit 11:     RETTOBASE (Return to base level)
    Bits 8:0:   VECTACTIVE (Active vector number)
    """
    scb_icsr = read_memory(SCB_ICSR_ADDR)
    vectactive = scb_icsr & 0x1FF
    vectpending = (scb_icsr >> 12) & 0x1FF
    isrpending = (scb_icsr >> 22) & 1
    print(f"  SCB_ICSR: 0x{scb_icsr:08X}")
    print(f"    VECTACTIVE: {vectactive} VECTPENDING: {vectpending} ISRPENDING: {isrpending}")

def read_and_check_nvic_iabrs():
    """Read and check NVIC_IABR* interrupt active registers from 0xE000E300
    onwards
    """
    print(f"  Read NVIC_IABR registers:")
    for i in range(NUM_NVIC_IABR_REG):
        iabr = read_memory(NVIC_IABR0_ADDR + (i * 4))
        if iabr:
            print(f"  NVIC_IABR{i}: 0x{iabr:08X}")
    print(f"  NVIC_IABR registers read complete.")

def read_dbgmcu_idcode():
    """Read and display DBGMCU_IDCODE register at 0xE0042000
    
    Bits 31:16: REV_ID (Revision identifier)
    Bits 11:0:  DEV_ID (Device identifier)
    """
    dbgmcu_idcode = read_memory(DBGMCU_IDCODE)
    rev_id = (dbgmcu_idcode >> 16) & 0xFFFF
    dev_id = dbgmcu_idcode & 0xFFF
    
    print(f"  DBGMCU_IDCODE: 0x{dbgmcu_idcode:08X}")
    print(f"    DEV_ID: 0x{dev_id:03X}")
    print(f"    REV_ID: 0x{rev_id:04X}")  

def read_system_state():
    """Read and display all system configuration registers"""
    print("Reading system configuration...")
    read_dbgmcu_idcode()
    read_flash_acr()
    read_rcc_cr()
    read_rcc_pllcfgr()
    read_rcc_cfgr()
    read_rcc_cir()
    read_rcc_ahb1enr()
    read_rcc_apb1enr()
    read_pwr_cr()
    read_pwr_csr()
    for port in 'ABC':
        print(f"  GPIO{port}:")
        read_gpio_mode(port)
        read_gpio_otype(port)
        read_gpio_pupd(port)
        read_gpio_speed(port)
    read_scb_icsr()
    read_scb_stcsr()
    read_and_check_nvic_iabrs()
    print("System state read complete!\n")
    print()

def read_dwt_state():
    """Read and display current DWT register states before modification"""
    print("  Reading current CoreDebug DEMCR...")
    current_demcr = read_memory(DEMCR_ADDR)
    trcena = (current_demcr >> 24) & 1
    print(f"    Current DEMCR: 0x{current_demcr:08X}")
    print(f"    TRCENA (bit 24): {trcena}")
    
    print("  Reading current DWT_CTRL...")
    current_dwt_ctrl = read_memory(DWT_CTRL_ADDR)
    cyccntena = current_dwt_ctrl & 1
    cpievtena = (current_dwt_ctrl >> 17) & 1
    excevtena = (current_dwt_ctrl >> 18) & 1
    sleepevtena = (current_dwt_ctrl >> 19) & 1
    lsuevtena = (current_dwt_ctrl >> 20) & 1
    foldevtena = (current_dwt_ctrl >> 21) & 1
    nocyccnt = (current_dwt_ctrl >> 25) & 1
    noprfcnt = (current_dwt_ctrl >> 24) & 1
    print(f"    Current DWT_CTRL: 0x{current_dwt_ctrl:08X}")
    print(f"    Enabled counters: CYC={cyccntena} CPI={cpievtena} EXC={excevtena} SLEEP={sleepevtena} LSU={lsuevtena} FOLD={foldevtena}")
    print(f"    Hardware support: CYCCNT={'NO' if nocyccnt else 'YES'} PROFCNT={'NO' if noprfcnt else 'YES'}")
    
    print("  Reading current counter values...")
    print(f"    CYCCNT:   0x{read_memory(DWT_CYCCNT_ADDR):08X}")
    print(f"    CPICNT:   0x{read_memory(DWT_CPICNT_ADDR):08X}")
    print(f"    EXCCNT:   0x{read_memory(DWT_EXCCNT_ADDR):08X}")
    print(f"    SLEEPCNT: 0x{read_memory(DWT_SLEEPCNT_ADDR):08X}")
    print(f"    LSUCNT:   0x{read_memory(DWT_LSUCNT_ADDR):08X}")
    print(f"    FOLDCNT:  0x{read_memory(DWT_FOLDCNT_ADDR):08X}")

def configure_dwt():
    """Configure DWT registers for performance monitoring"""
    # Enable TRCENA in CoreDebug DEMCR (bit 24)
    print("  Enabling TRCENA in CoreDebug DEMCR...")
    write_memory(DEMCR_ADDR, 0x01000000)
    print(f"  New DEMCR: 0x{read_memory(DEMCR_ADDR):08X}")

    # Configure DWT_CTRL to enable all performance counters
    print("  Enabling DWT counters...")
    dwt_ctrl_value = (1 << 21) | (1 << 20) | (1 << 19) | (1 << 18) | (1 << 17) | (1 << 0)
    print(f"    Updating DWT_CTRL: 0x{dwt_ctrl_value:08X}")
    write_memory(DWT_CTRL_ADDR, dwt_ctrl_value)
    print(f"  New DWT_CTRL: 0x{read_memory(DWT_CTRL_ADDR):08X}")

    # Clear all counters to start fresh
    print("  Clearing all counters...")
    write_memory(DWT_CYCCNT_ADDR, 0)
    write_memory(DWT_CPICNT_ADDR, 0)
    write_memory(DWT_EXCCNT_ADDR, 0)
    write_memory(DWT_SLEEPCNT_ADDR, 0)
    write_memory(DWT_LSUCNT_ADDR, 0)
    write_memory(DWT_FOLDCNT_ADDR, 0)

def check_wraps(current, previous):
    """Return string with * for wrapped counters"""
    if previous is None:
        return ""
    
    wraps = ""
    if (current['cpi_extra'] & 0xFF) < (previous['cpi_extra'] & 0xFF):
        wraps += "*"
    if (current['lsu'] & 0xFF) < (previous['lsu'] & 0xFF):
        wraps += "*"
    if (current['folded'] & 0xFF) < (previous['folded'] & 0xFF):
        wraps += "*"
    if (current['exception'] & 0xFF) < (previous['exception'] & 0xFF):
        wraps += "*"
    
    return wraps

def setup_dwt():
    """Initialize DWT performance counters"""
    print("Setting up DWT performance counters...")
    read_dwt_state()
    configure_dwt()
    print("DWT setup complete!\n")

def read_dwt_counters():
    """Read all DWT performance counters and return as dict"""
    return {
        'cycles': read_memory(DWT_CYCCNT_ADDR),
        'cpi_extra': read_memory(DWT_CPICNT_ADDR),        # Extra cycles beyond 1 per instruction
        'exception': read_memory(DWT_EXCCNT_ADDR),        # Cycles spent in exception overhead
        'sleep': read_memory(DWT_SLEEPCNT_ADDR),          # Cycles spent sleeping
        'lsu': read_memory(DWT_LSUCNT_ADDR),              # Load/store unit operations
        'folded': read_memory(DWT_FOLDCNT_ADDR),          # Instructions that were folded (optimized away)
        'rom_bytes': read_memory(0x20000008)
    }

def calculate_metrics(current, previous):
    """Calculate performance metrics from counter deltas"""
    if previous is None:
        return {}
    
    delta_cycles = current['cycles'] - previous['cycles']
    delta_rom_bytes = current['rom_bytes'] - previous['rom_bytes']
    
    # Handle 32-bit counter overflow
    if delta_cycles < 0:
        delta_cycles += (1 << 32)  # Add 2^32 to correct for wraparound
    if delta_rom_bytes < 0:
        delta_rom_bytes += (1 << 32)
        
    return {
        'delta_cycles': delta_cycles,
        'delta_rom_bytes': delta_rom_bytes
    }

def main():
    """Main monitoring loop"""
    global BASE_URL
    
    # Parse command line arguments
    parser = argparse.ArgumentParser(description='Monitor STM32 DWT performance counters via airfrog')
    parser.add_argument('airfrog_host', help='IP address or hostname of airfrog device (e.g., 192.168.4.1)')
    args = parser.parse_args()
    
    # Set up base URL from command line argument
    if not args.airfrog_host.startswith('http'):
        BASE_URL = f"http://{args.airfrog_host}/api"
    else:
        BASE_URL = f"{args.airfrog_host}/api"
    
    print(f"Connecting to airfrog at: {BASE_URL}")
    
    try:
        reset_target()
        read_system_state()
        setup_dwt()
        
        print("Monitoring DWT counters (Ctrl+C to stop)...")
        print("=" * 90)
        print(f"{'Time':>8} {'Cycles':>10} {'LSU':>6} {'Fold':>6} {'Exc':>6} {'Cycles/s':>10} {'CPS_avg':>8} {'RomB/s':>8} {'RB_avg':>8}")
        print("=" * 90)
        
        previous_counters = None
        previous_time = None
        start_time = time.time()
        
        total_cycles = 0
        total_cycles_per_sec = 0
        counter = 0
        last_elapse = 0
        total_rom_bytes_per_sec = 0
        
        while True:
            cycle_read_time = time.time()
            current_counters = read_dwt_counters()
            
            metrics = calculate_metrics(current_counters, previous_counters)
            
            if metrics:
                # Calculate timing
                actual_interval = cycle_read_time - previous_time
                cycles_per_second = metrics['delta_cycles'] / actual_interval
                rom_bytes_per_second = metrics['delta_rom_bytes'] / actual_interval
                
                # Update rolling averages
                counter += 1
                total_cycles += metrics['delta_cycles']
                
                total_cycles_per_sec += cycles_per_second
                total_rom_bytes_per_sec += rom_bytes_per_second
                
                cycles_per_sec_average = total_cycles_per_sec / counter
                rom_bytes_per_sec_average = total_rom_bytes_per_sec / counter
                
                elapsed = time.time() - start_time
                
                last_elapse = elapsed
                print(f"{elapsed:6.1f} "
                    f"{metrics['delta_cycles']:10d} "
                    f"{current_counters['lsu']:6d} "
                    f"{current_counters['folded']:6d} "
                    f"{current_counters['exception']:6d} "
                    f"{cycles_per_second:11.0f} "
                    f"{cycles_per_sec_average:8.0f} "
                    f"{rom_bytes_per_second:8.0f} "
                    f"{rom_bytes_per_sec_average:8.0f}")
            
            previous_counters = current_counters.copy()
            previous_time = cycle_read_time
            
    except KeyboardInterrupt:
        print("\nMonitoring stopped.")
    except Exception as e:
        print(f"Error: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()