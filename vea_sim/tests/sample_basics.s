; sample_basics.s
;
; A small hand-written program for the onestep simulator. It exercises the core
; ALU ops, both operand encoding modes (register vs. #decimal / 0xHEX immediate),
; and a cmp + bne countdown loop, then halts.
;
; Checkpoint comments are read by tests/testgen.py. A comment whose first token
; is  ;=  asserts register values:
;
;   <instr>          ;= rK=V ...     after this instruction retires
;   ;= at N: rK=V ...                after exactly N retired instructions
;   ;= final: rK=V ...               after the program halts
;
; V is signed decimal (-1, 246) or 0xHEX, compared as the raw 64-bit value.

        mov   r1, #10          ;= r1=10
        mov   r2, 0x100        ;= r2=256
        add   r3, r1, r2       ;= r3=266
        sub   r4, r2, r1       ;= r4=246
        and   r5, r2, 0xF0     ;= r5=0
        or    r6, r1, r2       ;= r6=266
        xor   r7, r3, r3       ;= r7=0
        not   r8, r7           ;= r8=-1
        shl   r9, r1, #4       ;= r9=160
        shr   r10, r2, #2      ;= r10=64
        mov   r11, #3          ;= r11=3

; countdown: r11 -> 0. ALU ops do not touch the flags, so the loop is driven by
; an explicit cmp. After 21 retired instructions the halt has executed.
loop:   sub   r11, r11, #1
        cmp   r11, #0
        bne   loop
        halt                  ;= at 21: r11=0

;= final: r1=10 r9=160 r11=0
