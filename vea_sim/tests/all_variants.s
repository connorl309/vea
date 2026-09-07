nop // nothing happens!
mov r0, 0xDEADBEEF  // r0 == 0xDEADBEEF onestep c=2
mov r1, r0          // r1 == 0xDEADBEEF onestep c=3
add r2, r0, #0      // r2 == 0xDEADBEEF onestep c=4
sub r5, r0, #0      // r5 == 0xDEADBEEF onestep c=5
and r3, r0, 0x10101010 // r3 == 0x10001000 c=6
or r4, r3, 0x0F0F0F0F // r4 == 0x1F1F1F1F c=7
and r6, r6, 0xFFFFFFFF
not r6, r6 // r6 == E0E0_E0E0 c=9
xor r7, r6, r0 // r7 == 3E4D_5E04 c=10
shl r8, r7, 1 // r8 == 7C9A_BC1E c=11
shr r9, r8, 2 // r9 == 1F26_AF07 c=12
// ignoring sar...
mul r10, r9, r0 // r10 == 1B18_B028_24BF_9989
div r11, r10, 0x10000 // r11 == 1B18_B028_24BF
