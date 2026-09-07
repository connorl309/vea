; signed_vs_unsigned.s - the same operand pair (-5, 3) compared both ways.
; cmp.s treats -5 as negative, so blt is taken. cmp treats -5 as a huge
; unsigned value, so the same blt is not taken.

        mov   r1, -#5        ;= r1=-5
        mov   r2, #3         ;= r2=3

        cmp.s r1, r2
        blt   s_less
        mov   r10, #0
        b     after_s
s_less: mov   r10, #1
after_s:

        cmp   r1, r2
        blt   u_less
        mov   r11, #0
        b     after_u
u_less: mov   r11, #1
after_u:
        halt

;= final: r10=1 r11=0 r1=-5
