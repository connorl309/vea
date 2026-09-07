; memcpy.s - seed four 4-byte words at 0x1000, then copy them to 0x2000 with an
; index-register addressed loop (ld.w / st.w [base + rN]), then read the
; destination back.

        mov   r1, 0x1000      ;= r1=4096     ; src base
        mov   r2, 0x2000      ;= r2=8192     ; dst base

        mov   r5, 0x1111
        st.w  [r1], r5
        mov   r5, 0x2222
        st.w  [r1 + #4], r5
        mov   r5, 0x3333
        st.w  [r1 + #8], r5
        mov   r5, 0x4444
        st.w  [r1 + #12], r5

        mov   r3, #0          ; byte offset
        mov   r4, #16         ; bytes to copy
copy:   ld.w  r6, [r1 + r3]
        st.w  [r2 + r3], r6
        add   r3, r3, #4
        cmp   r3, r4
        blt   copy

        ld.w  r10, [r2]
        ld.w  r11, [r2 + #4]
        ld.w  r12, [r2 + #8]
        ld.w  r13, [r2 + #12]
        halt

;= final: r3=16 r10=0x1111 r11=0x2222 r12=0x3333 r13=0x4444
