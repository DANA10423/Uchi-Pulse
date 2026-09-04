/* Raspberry Pi Pico W / RP2040, 2 MiB flash and 264 KiB RAM. */
MEMORY {
    BOOT2 : ORIGIN = 0x10000000, LENGTH = 0x100
    /* Reserve the final 4 KiB erase sector for node configuration. */
    FLASH : ORIGIN = 0x10000100, LENGTH = 2048K - 0x100 - 0x1000
    RAM   : ORIGIN = 0x20000000, LENGTH = 264K
}
