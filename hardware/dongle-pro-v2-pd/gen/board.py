#!/usr/bin/env python3
"""Dongle-Pro v2 PD -> single-sheet wired KiCad schematic, in functional clusters.

Dongle-Pro v2 plus a charger port: a third USB-C receptacle takes a PD charger
and a TPS2121 power mux puts its VBUS (up to 20 V) on the target port, so the
target charges while it restores. CH224Q is the PD sink toward the charger.

USB 3 passthrough dongle. No hub in the SuperSpeed path: the SS
lanes run host receptacle -> CH9441P -> CH9441P -> target receptacle, each
CH9441P picking the live lane pair from its port's CC pins. USB 2.0 goes
through a CH334P hub (DS1 = MCU, DS2 = target). CH32X035G8U6 runs the dongle,
HUSB311 (TCPCI) owns the target CC for the Apple DFU VDMs.

BOM-row discipline: resistors are only 5.1k / 5.6k / 6.8k / 10k / 100k / 200k, caps
100nF / 1uF, 330nF on the target lanes, 4.7uF and 10uF 50 V on the VBUS rails.
"""
import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "dongle-lite", "gen"))
import gen
R={"5.1k":"0402WGF5101TCE","10k":"0402WGF1002TCE","5.6k":"0402WGF5601TCE","6.8k":"0402WGF6801TCE","100k":"0402WGF1003TCE","200k":"0402WGF2003TCE"}
C={"100nF":"CL05B104KO5NNNC","1uF":"CL05A105KA5NQNC","10uF":"GRM21BR61H106KE43L","330nF":"TCC0402X5R334K250AT","4.7uF":"CL21A475KBQNNNE","220pF":"0402B221K500NT"}
XTAL="X322512MSB4SI"; LED="KT-0603R"; BTN="TS3315A"
# receptacle: hanxia C5178545, rated 20 V 5 A. Same footprint as the v2's JXTCONN part, which is rated 5 V.
USBC="HX-TYPE-C-24P-QCHT"; ESD="H5VU25U"; MUX="CH9441P"; FET="HL2302"; FET30="AO3400A"
comps=[]
def add(ref,sym,at,nets,value=None): comps.append({"ref":ref,"sym":sym,"at":at,"nets":nets,"value":value})
def r(ref,val,at,a,b): add(ref,R[val],at,{"1":a,"2":b},val)
def c(ref,val,at,a,b): add(ref,C[val],at,{"1":a,"2":b},val)
def esd(ref,at,n1,n2,n4,n5):
    # a None channel is left unconnected (both of its pads)
    # SS arrays: which lane pair takes which channel, and P/N order, follow the layout
    # RClamp0524P pinout: I/O on 1,2,4,5 with flow-through partners 10,9,7,6; GND 3,8
    add(ref,ESD,at,{"1":n1,"10":n1,"2":n2,"9":n2,"4":n4,"7":n4,"5":n5,"6":n5,"3":"GND","8":"GND"})

# ===== MCU cluster — CH32X035G8U6 (QFN-28, 48 MHz HSI, no crystal) =====
# Firmware pin map (chosen so every net leaves the QFN without a crossover):
#   PB10 I2C_SDA / PB9 I2C_SCL  (bit-banged: the QFN-28 only bonds hardware
#        I2C on the USB and SDI pins)
#   PB8  PD_INT (HUSB311 INT_N)
#   PB6  SBU_PU (drive high to enable the SBU level shifters, Hi-Z/low = off)
#   PA1  LED_STAT
#   SBU1_MCU = PA2 (USART2 TX) + PB4 (USART3 RX)
#   SBU2_MCU = PA3 (USART2 RX) + PB3 (USART3 TX)
#        normal cable: USART2 (TX on SBU1, RX on SBU2); flipped: USART3. The
#        idle USART's TX pin stays an input; enable the internal pull-up on
#        whichever pin is receiving. PA4..PB0 between them stay unused so the
#        two SBU nets can wrap the east side of the package.
#   PA4  CHG_SCL / PA5 CHG_SDA  (second bit-banged I2C bus, CH224Q at 0x22/0x23;
#        the CH224Q has its own 10k pull-ups, hold both low or Hi-Z while no
#        charger is plugged in)
#   PA6  HOST_EN_N (drive low to pass host 5 V to the target; Hi-Z = off)
#   PA7  CHG_EN (high = pass charger VBUS to the target while it is under ~6.4 V;
#        10k pulldown)
#   PB0  CHG_HV (high = pass it at any voltage; 10k pulldown. Raise only once
#        the target has accepted a contract above 5 V)
#   PC16/PC17 USB D-/D+   PC18/PC19 SDI debug (TP9/TP10)
#   PC17 high at power-on = ROM USB bootloader
add("U1","CH32X035G8U6",(150,150,0),{
 "2":"+3V3","29":"GND",
 "21":"I2C_SDA","20":"I2C_SCL","19":"PD_INT","17":"SBU_PU",
 "9":"CHG_SCL","10":"CHG_SDA","11":"HOST_EN_N","12":"CHG_EN","13":"CHG_HV",
 "6":"LED_STAT",
 "7":"SBU1_MCU","8":"SBU2_MCU","14":"SBU2_MCU","15":"SBU1_MCU",
 "24":"SWCLK","25":"SWDIO","26":"MCU_D_N","27":"MCU_D_P",
 "1":None,"3":None,"4":None,"5":None,
 "16":None,"18":None,"22":None,"23":None,"28":None},"CH32X035G8U6")
c("C1","100nF",(95,230,0),"+3V3","GND"); c("C2","1uF",(120,230,0),"+3V3","GND")
# BOOT: hold SW1 while plugging in -> PC17 (D+) high at power-on -> ROM USB ISP
add("SW1",BTN,(120,78,0),{"1":"BOOT_SW","2":"BOOT_SW","3":"+3V3","4":"+3V3"})
r("R1","5.1k",(160,78,0),"BOOT_SW","MCU_D_P")

# ===== USB-PD cluster — HUSB311 TCPC on the target CC =====
add("U2","HUSB311BLA",(150,330,0),{
 "3":"+3V3","4":"+3V3","9":"GND","8":"GND","15":"GND",
 "10":"TGT_CC1","11":"TGT_CC1","1":"TGT_CC2","14":"TGT_CC2",
 "2":"TGT_VBUS","12":None,"13":None,
 "5":"PD_INT","6":"I2C_SCL","7":"I2C_SDA"})
# no external pull-ups: SDA/SCL/INT_N use the MCU's internal ones (bit-banged, ~100 kHz)
c("C3","1uF",(190,380,0),"+3V3","GND")
# PD wants 200..600 pF on each CC line at a port; the HUSB311's edge rates are specified into 200 pF
c("C22","220pF",(120,400,0),"TGT_CC1","GND"); c("C23","220pF",(100,430,0),"TGT_CC2","GND")

# ===== Host port cluster =====
add("J1",USBC,(360,150,0),{
 "A1":"GND","A12":"GND","B1":"GND","B12":"GND","25":"GND",
 # VBUS on the right pair only (A4/B9), same reasoning as the target port
 "A4":"+5V","B9":"+5V","A9":None,"B4":None,
 "A5":"HOST_CC1","B5":"HOST_CC2",
 "A6":"HOST_D_P","A7":"HOST_D_N","B6":"HOST_D_P","B7":"HOST_D_N",
 "A8":None,"B8":None,
 "A2":"HOST_TX1_P","A3":"HOST_TX1_N","B11":"HOST_RX1_P","B10":"HOST_RX1_N",
 "B2":"HOST_TX2_P","B3":"HOST_TX2_N","A11":"HOST_RX2_P","A10":"HOST_RX2_N"})
r("R6","5.1k",(330,250,0),"HOST_CC1","GND"); r("R7","5.1k",(355,250,0),"HOST_CC2","GND")
# Host lane mux: CC1 live -> lane 1 (TX1/RX1), CC2 live -> lane 2.
# SSA carries target->host (host receptacle TX pins), SSB host->target.
# Pin choice is layout-driven: the A-row (SMD) pair of each lane lands on the
# mux's connector-facing pins, the B-row (through-hole) pair on its side pins,
# and P/N follow whichever order routes without a crossover. USB 3 receivers
# correct lane polarity themselves, so a lane may invert through the mux.
add("U3",MUX,(470,150,0),{
 "13":"HOST_CC1","14":"HOST_CC2","8":"+5V","7":"HMUX_VDD","17":"GND",
 "1":"HOST_TX1_P","2":"HOST_TX1_N","15":"HOST_RX1_P","16":"HOST_RX1_N",
 "3":"HOST_RX2_N","4":"HOST_RX2_P","5":"HOST_TX2_N","6":"HOST_TX2_P",
 "9":"SSA_P","10":"SSA_N","11":"SSB_P","12":"SSB_N"})
c("C5","100nF",(470,230,0),"HMUX_VDD","GND"); c("C6","1uF",(500,230,0),"+5V","GND")
# USB 2.0 ESD sits at the hub pins (the D+/D- trunks run on B.Cu); two channels spare
esd("D10",(330,300,0),"HOST_D_N","HOST_D_P",None,None)
esd("D12",(400,300,0),"HOST_TX1_N","HOST_TX1_P","HOST_RX1_N","HOST_RX1_P")
esd("D13",(470,300,0),"HOST_TX2_P","HOST_TX2_N","HOST_RX2_P","HOST_RX2_N")

# ===== USB 2.0 hub cluster — CH334P =====
# No PSELF pin on this package; built-in default config is self-powered, which
# is what lets the restore-mode device take its 500 mA. RESET#/CDP and PGANG
# stay open (internal pull-ups).
add("U5","CH334P",(650,150,0),{
 # downstream port 1 (east pins) = target, port 2 (south pins) = MCU: layout-driven
 "12":"HOST_D_P","11":"HOST_D_N","10":"TGT_D_P","9":"TGT_D_N","8":"MCU_D_P","7":"MCU_D_N",
 "3":None,"4":None,"5":None,"6":None,"13":None,"14":None,
 "1":"HUB_XO","2":"HUB_XI","15":"+5V","16":"HUB_3V3","17":"GND"})
add("Y1",XTAL,(650,250,0),{"1":"HUB_XI","3":"HUB_XO","2":"GND","4":"GND"})  # XI/XO carry ~16pF internally
c("C7","1uF",(600,300,0),"+5V","GND"); c("C8","1uF",(630,300,0),"HUB_3V3","GND")

# ===== Target port cluster =====
# Target lane mux mirrors the host one: SSA comes from the target RX pins,
# SSB drives the target TX pins. Its VDD is fed straight from +3V3 (2.7-5.5 V
# external supply is allowed) with the LDO input tied to the same rail, which
# keeps +5V out of the bottom half of the board.
add("U4",MUX,(830,150,0),{
 "13":"TMUX_CC1","14":"TMUX_CC2","8":"+3V3","7":"+3V3","17":"GND",
 "1":"TGT_TX1_P","2":"TGT_TX1_N","15":"TGT_RX1_P","16":"TGT_RX1_N",
 "3":"TGT_RX2_N","4":"TGT_RX2_P","5":"TGT_TX2_N","6":"TGT_TX2_P",
 "12":"SSA_P","11":"SSA_N","10":"SSB_P","9":"SSB_N"})
c("C10","100nF",(830,230,0),"+3V3","GND")
# The mux's CC inputs are 6 V pins and only sense the level, so they sit behind
# 10k: a VBUS-to-CC short at 20 V pushes 1.5 mA into their ESD clamps, and 1 uA
# of input current only moves the level 10 mV. HUSB311 CC is 24 V rated.
r("R30","10k",(800,600,0),"TGT_CC1","TMUX_CC1"); r("R31","10k",(830,600,0),"TGT_CC2","TMUX_CC2")
add("J2",USBC,(950,150,0),{
 "A1":"GND","A12":"GND","B1":"GND","B12":"GND","25":"GND",
 # all four VBUS contacts: this port carries up to 3 A at 20 V
 "A4":"TGT_VBUS","B9":"TGT_VBUS","A9":"TGT_VBUS","B4":"TGT_VBUS",
 "A5":"TGT_CC1","B5":"TGT_CC2",
 "A6":"TGT_D_P","A7":"TGT_D_N","B6":"TGT_D_P","B7":"TGT_D_N",
 "A8":"TGT_SBU1","B8":"TGT_SBU2",
 "A2":"TGTC_TX1_P","A3":"TGTC_TX1_N","B11":"TGTC_RX1_P","B10":"TGTC_RX1_N",
 "B2":"TGTC_TX2_P","B3":"TGTC_TX2_N","A11":"TGTC_RX2_P","A10":"TGTC_RX2_N"})
# 20 V protection for the lanes (VBUS sits next to TX-/RX- in the receptacle):
# 330nF in series with every lane pin, as the Type-C spec does for RX, so a
# short only reaches the TVS arrays as a pulse. 200k on the connector side of
# each cap bleeds it afterwards, otherwise it would dump into the next Mac.
for i,ln in enumerate(("RX1_P","RX1_N","TX1_P","TX1_N","RX2_P","RX2_N","TX2_P","TX2_N")):
    c(f"C{30+i}","330nF",(760+i*28,650,0),f"TGTC_{ln}",f"TGT_{ln}")
    r(f"R{60+i}","200k",(760+i*28,690,0),f"TGTC_{ln}","GND")
esd("D11",(800,300,0),None,None,"TGT_D_N","TGT_D_P")
esd("D14",(870,300,0),"TGT_RX1_P","TGT_RX1_N","TGT_TX1_P","TGT_TX1_N")
esd("D15",(940,300,0),"TGT_RX2_N","TGT_RX2_P","TGT_TX2_N","TGT_TX2_P")

# ===== SBU serial cluster — 1.2 V <-> 3.3 V, one common-gate FET per line =====
# SBU_PU high: 10k/5.6k holds each SBU line at 1.18 V idle and the gate sits at
# 1.65 V, so the FET conducts only when either side pulls low and can never
# lift SBU above ~1.1 V. SBU_PU low or Hi-Z: gate at 0 V, FETs off, and each SBU
# line is left with 5.6k (low) or 15.6k (Hi-Z) to ground. The MCU side idles
# high on the MCU's internal pull-ups.
# 20 V protection (VBUS sits next to SBU in the receptacle): a 30 V FET in
# series with each line, gate at 3.3 V (5.1k/10k from +5V). It passes the 1.2 V
# signal and pinches off above about 2 V, so a short puts nothing into the
# shifter or the MCU and can be held indefinitely.
add("Q1",FET,(360,430,0),{"1":"SBU_VG","2":"SBU1_P","3":"SBU1_MCU"})
add("Q2",FET,(440,430,0),{"1":"SBU_VG","2":"SBU2_P","3":"SBU2_MCU"})
r("R8","10k",(330,470,0),"SBU_PU","SBU1_P"); r("R9","5.6k",(355,470,0),"SBU1_P","GND")
r("R10","10k",(410,470,0),"SBU_PU","SBU2_P"); r("R11","5.6k",(435,470,0),"SBU2_P","GND")
add("Q5",FET30,(340,620,0),{"1":"SBU_G","2":"SBU1_P","3":"TGT_SBU1"})
add("Q6",FET30,(420,620,0),{"1":"SBU_G","2":"SBU2_P","3":"TGT_SBU2"})
r("R46","5.1k",(480,600,0),"+5V","SBU_G"); r("R47","10k",(510,600,0),"SBU_G","GND")
c("C21","100nF",(540,600,0),"SBU_G","GND")   # holds the gates still when a drain jumps to 20 V
r("R12","10k",(330,500,0),"SBU_PU","SBU_VG"); r("R13","10k",(355,500,0),"SBU_VG","GND")
c("C12","100nF",(380,500,0),"SBU_VG","GND")

# ===== Power cluster =====
add("U6","ME6211C33M5G-N",(560,430,0),{"1":"+5V","2":"GND","3":"+5V","4":None,"5":"+3V3"})
c("C13","1uF",(530,470,0),"+5V","GND"); c("C14","1uF",(560,470,0),"+3V3","GND")
# Target VBUS: TPS2121 power mux. IN1 = host 5 V, IN2 = charger VBUS (5..20 V),
# PR1 = CP2 = GND so the higher valid input wins (IN2 on a tie). Each input is
# switched off by holding its OV pin above 1.06 V:
#   OV1 (host)    10k/5.1k from +5V = 1.69 V; the MCU pulls HOST_EN_N low to enable
#   OV2 (charger) 20k/6.8k from VBUS_IN = 1.2 V at 4.75 V, 5.3 V at 21 V, so the
#                 charger input is off by default, MCU powered or not.
#                 CHG_EN: Q4 loads OV2 with 200k, which turns the off switch
#                 into a 6.4 V overvoltage trip: 5 V passes, the CH224Q's 9 V
#                 power-up default and anything above it does not.
#                 CHG_HV: Q3 shorts OV2 to ground, any voltage passes.
# CP2 follows charger VBUS (same divider, 100k isolates it from Q3): with the
# charger above ~4.5 V it has priority over the host whatever the host voltage
# is, and switchover is the fast kind (5 us instead of 100 us).
# ILM: 80k to ground (100k || 400k) = 1.5 A while the host feeds the target. ST
# goes low when the charger is selected and adds 40k in parallel = 3.9 A.
# SS 100nF keeps the 20 V turn-on slow.
add("U7","TPS2121RUXR",(660,430,0),{"1":"TGT_VBUS","8":"TGT_VBUS","7":"+5V","2":"VBUS_IN",
 "6":"GND","3":"CHG_SNS","9":"MUX_ST","12":"GND","10":"MUX_ILM","11":"MUX_SS","5":"HOST_EN_N","4":"CHG_OV"})
r("R16","10k",(620,480,0),"MUX_ILM","MUX_ILM_A"); r("R19","10k",(645,480,0),"MUX_ILM_A","MUX_ILM_B")
r("R20","10k",(670,480,0),"MUX_ILM_B","MUX_ILM_C"); r("R34","10k",(560,600,0),"MUX_ST","MUX_ILM_C")
r("R35","100k",(590,600,0),"GND","MUX_ILM")
r("R48","200k",(590,640,0),"MUX_ILM","MUX_ILM_D"); r("R49","200k",(620,640,0),"MUX_ILM_D","GND")
c("C18","100nF",(695,480,0),"MUX_SS","GND")
# 5k bleeder (2x 10k): with the mux off, 20 uF falls from 20 V to under 0.8 V in ~0.3 s
r("R27","10k",(745,525,0),"TGT_VBUS","GND"); r("R42","10k",(620,600,0),"TGT_VBUS","GND")
c("C20","10uF",(660,610,0),"TGT_VBUS","GND")
c("C15","1uF",(680,525,0),"+5V","GND"); c("C17","10uF",(720,525,0),"TGT_VBUS","GND")
r("R2","10k",(730,430,0),"+5V","HOST_EN_N"); r("R21","5.1k",(730,460,0),"HOST_EN_N","GND")
add("D1",LED,(530,510,0),{"1":"LEDP_A","2":"GND"}); r("R17","5.1k",(560,510,0),"+5V","LEDP_A")
add("D2",LED,(600,510,0),{"1":"LEDS_A","2":"GND"}); r("R18","5.1k",(630,510,0),"LED_STAT","LEDS_A")
for i,rail in enumerate(["+5V","+3V3","GND","HUB_3V3","TGT_VBUS","HMUX_VDD","VBUS_IN"]):
    add(f"#FLG{i+1}","PWR_FLAG",(520+i*22,550,0),{"1":rail},"PWR_FLAG")

# ===== Charger port cluster =====
# Same receptacle as the other two ports (one less BOM line), power pins only.
# CH224Q is a PD sink with its own Rd and 32 V
# tolerant CC pins; the MCU reads the charger's source caps from it over I2C
# (0x60..0x8F) and picks 5/9/12/15/20 V (reg 0x0A). CFG1 6.8k = 9 V until the MCU
# says otherwise, which also turns its I2C on. A sink does not care which CC is
# which, so CC1/CC2 are wired whichever way lays out without a crossover.
add("J3",USBC,(1050,150,0),{
 "A1":"GND","A12":"GND","B1":"GND","B12":"GND","25":"GND",
 "A4":"VBUS_IN","B9":"VBUS_IN","A9":"VBUS_IN","B4":"VBUS_IN",
 "A5":"CHG_CC2","B5":"CHG_CC1",
 "A6":None,"A7":None,"B6":None,"B7":None,"A8":None,"B8":None,
 "A2":None,"A3":None,"B11":None,"B10":None,"B2":None,"B3":None,"A11":None,"A10":None})
add("U8","CH224Q",(1050,330,0),{"1":"VBUS_IN","8":"VBUS_IN","11":"GND",
 "2":"CHG_SCL","3":"CHG_SDA","4":"CHG_DP","5":"CHG_DM","6":"CHG_CC2","7":"CHG_CC1","9":"CHG_CFG","10":None})
r("R26","6.8k",(1100,370,0),"CHG_CFG","GND")
c("C19","1uF",(1130,370,0),"VBUS_IN","GND")   # CH224Q supply; 25 V part, sees up to 21 V
c("C16","4.7uF",(1010,370,0),"VBUS_IN","GND")   # with C19 stays under USB's 10 uF sink limit
r("R22","10k",(1010,420,0),"VBUS_IN","CHG_OV_A"); r("R23","10k",(1035,420,0),"CHG_OV_A","CHG_SNS")
r("R24","6.8k",(1060,420,0),"CHG_SNS","GND"); r("R39","100k",(1060,470,0),"CHG_SNS","CHG_OV")
# BC1.2 pins: PD only, held low instead of left floating
r("R40","10k",(1010,520,0),"CHG_DP","GND"); r("R41","10k",(1040,520,0),"CHG_DM","GND")
add("Q3",FET,(1100,450,0),{"1":"CHG_HV","2":"GND","3":"CHG_OV"})
r("R25","10k",(1130,420,0),"CHG_HV","GND")
add("Q4",FET,(1100,560,0),{"1":"CHG_EN","2":"GND","3":"CHG_LIM"})
r("R45","10k",(1130,600,0),"CHG_EN","GND")
r("R43","200k",(1010,600,0),"CHG_OV","CHG_LIM")

# ===== Debug / bring-up cluster =====
# SDI debug (WCH-LinkE: SWDIO, SWCLK, 3V3, GND) three ways: pads next to the MCU,
# a row of four through-holes at 2.54 mm for a pin header (not fitted), and a
# TC2030 Tag-Connect footprint with the same pinout as the other dongles
# (1 VTREF, 2 SWDIO, 4 SWCLK, 5 GND; 3 and 6 are open, this MCU has no reset pin).
comps.append({"ref":"J4","sym":"TC2030","at":(900,480,0),"in_bom":False,"value":"TC2030-IDC-NL",
              "nets":{"1":"+3V3","2":"SWDIO","3":None,"4":"SWCLK","5":"GND","6":None}})
# probe pads for bring-up: both VBUS rails and host 5 V, the target CC lines (PD
# sniffing), the charger I2C bus, the charger off-switch node and its two
# enables' neighbours, and the SBU lines on the connector side plus their gate bias
for i,(ref,net) in enumerate([("TP20","TGT_VBUS"),("TP21","VBUS_IN"),("TP22","+5V"),("TP23","TGT_CC1"),("TP24","TGT_CC2"),
                              ("TP25","CHG_SCL"),("TP26","CHG_SDA"),("TP27","CHG_OV"),("TP28","CHG_HV"),
                              ("TP30","TGT_SBU1"),("TP31","TGT_SBU2"),("TP32","SBU_G")]):
    comps.append({"ref":ref,"sym":"TP","at":(760+(i%7)*20,740+(i//7)*25,0),"in_bom":False,"value":net,"nets":{"1":net}})
for i,(ref,net) in enumerate([("TP11","SWCLK"),("TP12","SWDIO"),("TP13","+3V3"),("TP14","GND")]):
    comps.append({"ref":ref,"sym":"TPTH","at":(900+i*20,540,0),"in_bom":False,"value":net,"nets":{"1":net}})
for i,(ref,net) in enumerate([("TP2","+3V3"),("TP5","GND"),
                              ("TP9","SWCLK"),("TP10","SWDIO")]):
    comps.append({"ref":ref,"sym":"TP","at":(760+(i%4)*20,480+(i//4)*25,0),
                  "in_bom":False,"value":net,"nets":{"1":net}})

SS_PAIRS={f"{p}_{s}" for p in
  ("SSA","SSB","HOST_TX1","HOST_TX2","HOST_RX1","HOST_RX2",
   "TGT_TX1","TGT_TX2","TGT_RX1","TGT_RX2","TGTC_TX1","TGTC_TX2","TGTC_RX1","TGTC_RX2")
  for s in ("P","N")}
gen.build_wired(comps,"dongle-pro-v2-pd.kicad_sch",
                {"GND":"GND","+3V3":"+3V3","+5V":"+5V"},
                wires=[], auto_wire_dist=90, paper="A0", project="dongle-pro-v2-pd",
                force_label=SS_PAIRS|{"HOST_D_P","HOST_D_N","TGT_D_P","TGT_D_N","MCU_D_P","MCU_D_N",
                                      "HOST_CC1","HOST_CC2","TGT_CC1","TGT_CC2","TGT_SBU1","TGT_SBU2",
                                      "I2C_SCL","I2C_SDA","PD_INT","SBU1_MCU","SBU2_MCU","SBU_PU","SBU_VG","SWCLK","SWDIO","LED_STAT","BOOT_SW","LEDP_A","LEDS_A","HUB_XI","HUB_XO","HMUX_VDD","HUB_3V3",
                                      "TGT_VBUS","VBUS_IN","HOST_EN_N","CHG_EN","CHG_OV","CHG_OV_A","CHG_SCL","CHG_SDA","CHG_CC1","CHG_CC2","CHG_CFG",
                                      "MUX_ILM","MUX_ILM_A","MUX_ILM_B","MUX_ILM_C","MUX_ST","MUX_SS","CHG_SNS","CHG_DP","CHG_DM","CHG_HV","CHG_LIM",
                                      "TMUX_CC1","TMUX_CC2","SBU1_P","SBU2_P","SBU_G","MUX_ILM_D"})
print(f"{len(comps)} comps")
