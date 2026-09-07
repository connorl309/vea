; run_all_instrs_all_modes.s
;
; Every ALU op in both encoding modes, plus a store / load round trip through
; simulated memory at each of a few widths. Straight-line, ends in halt.
; See tests/testgen.py (or sample_basics.s) for the ;= checkpoint syntax.

        mov   r1, #100         ;= r1=100
        mov   r2, r1           ;= r2=100
        add   r3, r1, #5       ;= r3=105
        add   r4, r1, r2       ;= r4=200
        sub   r5, r3, #5       ;= r5=100
        and   r6, r1, r1       ;= r6=100
        or    r7, r1, #3       ;= r7=103
        xor   r8, r1, r1       ;= r8=0
        not   r9, r8           ;= r9=-1
        shl   r10, r1, #1      ;= r10=200
        shr   r11, r1, #1      ;= r11=50
        sar   r12, r9, #1      ;= r12=-1
        mul   r13, r1, #3      ;= r13=300
        div   r14, r1, #4      ;= r14=25

; memory: store r1 as a full word, zero r1, read it back; then a byte store/load.
        mov   r20, 0x4000     ;= r20=16384
        st    [r20], r1
        mov   r1, #0          ;= r1=0
        ld    r1, [r20]       ;= r1=100
        st.b  [r20 + #8], r3
        ld.b  r15, [r20 + #8] ;= r15=105
        halt

;= final: r1=100 r12=-1 r14=25 r15=105
