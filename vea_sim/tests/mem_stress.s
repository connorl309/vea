; mem_stress.s
; every access width, big-endian byte order, displacement / index / negative
; addressing, partial overwrites of a wider region, sign- vs zero-extended
; narrow loads, and a store that straddles the 64 KiB page boundary.
;
; ;= mem@ADDR = [bytes] reads sim memory byte for byte (unwritten bytes are 0).

; ---- 1. full 8-byte store, check the big-endian layout -------------------
        mov   r1, 0x1000
        mov   r2, 0x1122334455667788
        st    [r1], r2          ;= mem@0x1000 = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]

; ---- 2. 2-byte store overwrites the middle, the rest stays put ----------
        mov   r3, 0xAABB
        st.h  [r1 + #2], r3     ;= mem@0x1000 = [0x11, 0x22, 0xAA, 0xBB, 0x55, 0x66, 0x77, 0x88]

; ---- 3. 4-byte store at +4 (high two bytes of the value are zero) -------
        mov   r4, 0xC0DE
        st.w  [r1 + #4], r4     ;= mem@0x1000 = [0x11, 0x22, 0xAA, 0xBB, 0x00, 0x00, 0xC0, 0xDE]

; ---- 4. single byte, then a negative displacement ----------------------
        mov   r5, 0x2010
        mov   r6, 0x7F
        st.b  [r5], r6
        mov   r6, #1
        st.b  [r5 - #1], r6     ;= mem@0x200F = [0x01, 0x7F]

; ---- 5. index-register addressing -------------------------------------
        mov   r7, 0x3000
        mov   r8, #8
        mov   r9, 0x0BADF00D
        st.w  [r7 + r8], r9     ;= mem@0x3008 = [0x0B, 0xAD, 0xF0, 0x0D]

; ---- 6. sign-extended vs zero-extended narrow loads ------------------
        mov   r10, 0x4000
        mov   r11, 0xFF
        st.b  [r10], r11
        ld.b  r12, [r10]        ;= r12=255
        ld.sb r13, [r10]        ;= r13=-1
        mov   r14, 0x80
        st.h  [r10 + #4], r14
        ld.sh r15, [r10 + #4]   ;= r15=128
        mov   r16, 0x8000
        st.h  [r10 + #8], r16
        ld.sh r17, [r10 + #8]   ;= r17=-32768

; ---- 7. an 8-byte access that crosses the 0x10000 page boundary -----
        mov   r18, 0xFFFC
        mov   r19, 0x0102030405060708
        st    [r18], r19       ;= mem@0xFFFC = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]
        ld    r20, [r18]      ;= r20=0x0102030405060708

        halt

;= final: r12=255 r13=-1 r15=128 r17=-32768 r20=0x0102030405060708
;= final: mem@0x1000 = [0x11, 0x22, 0xAA, 0xBB, 0x00, 0x00, 0xC0, 0xDE]
;= final: mem@0x200F = [0x01, 0x7F]
;= final: mem@0x3008 = [0x0B, 0xAD, 0xF0, 0x0D]
; the tail of the page-crossing store, then two bytes that were never written
;= final: mem@0xFFFE = [0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x00, 0x00]
; a region nothing ever touched
;= final: mem@0x5000 = [0, 0, 0, 0, 0, 0, 0, 0]
