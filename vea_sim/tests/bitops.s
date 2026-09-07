; bitops.s - straight-line bit twiddling. Covers and/or/xor/not and the three
; shifts, including the logical (shr) vs arithmetic (sar) distinction on a
; negative value.

        mov   r1, 0x0F0F      ;= r1=3855
        mov   r2, 0x00FF      ;= r2=255
        and   r3, r1, r2      ;= r3=0x000F
        or    r4, r1, r2      ;= r4=0x0FFF
        xor   r5, r1, r2      ;= r5=0x0FF0
        not   r6, r1          ;= r6=-3856
        shl   r7, r2, #8      ;= r7=0xFF00
        shr   r8, r7, #4      ;= r8=0x0FF0
        mov   r9, -#16        ;= r9=-16
        sar   r10, r9, #2     ;= r10=-4
        shr   r11, r9, #2     ;= r11=0x3FFFFFFFFFFFFFFC
        halt

;= final: r3=15 r6=-3856 r10=-4
