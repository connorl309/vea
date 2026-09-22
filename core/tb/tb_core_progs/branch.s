        mov   r11, #5
loop:   sub   r11, r11, #1
        cmp   r11, #0
        bne   loop
        halt
