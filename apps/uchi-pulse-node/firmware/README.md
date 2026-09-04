# CYW43439 firmware blobs

The `cyw43` driver needs the official firmware files at build time. Place these
files in this directory before building the firmware:

- `43439A0.bin`
- `43439A0_clm.bin`
- `nvram_rp2040.bin` for Pico W
- `nvram_rp235x.bin` for Pico 2 W

Use the matching files from the Embassy `cyw43-firmware` package. They are
intentionally not committed to this repository.

The node configuration uses the final 4 KiB erase sector of the configured
2 MiB flash image. The linker scripts reserve this sector from the firmware
image for both RP2040 and RP2350; it is erased independently by the CDC
`factory_reset` operation. This is configuration storage only, not a whole
flash erase.
