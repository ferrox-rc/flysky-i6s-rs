MEMORY
{
  /* 120 KB On-chip Flash memory (Pages 0..59); Pages 60..63 (8 KB at 0x0801_E000) reserved for sequential storage */
  FLASH (rx)  : ORIGIN = 0x08000000, LENGTH = 120K

  /* 16 KB On-chip SRAM */
  RAM   (xrw) : ORIGIN = 0x20000000, LENGTH = 16K
}

/* Reserve stack at the top of RAM */
_stack_start = ORIGIN(RAM) + LENGTH(RAM);
