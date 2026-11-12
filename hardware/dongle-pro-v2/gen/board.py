#!/usr/bin/env python3
"""Dongle-Pro v2 -> single-sheet wired KiCad schematic, in functional clusters.

Cost-reduced USB 3 passthrough dongle. No hub in the SuperSpeed path: the SS
lanes run host receptacle -> CH9441P -> CH9441P -> target receptacle, each
CH9441P picking the live lane pair from its port's CC pins. USB 2.0 goes
through a CH334P hub (DS1 = MCU, DS2 = target). CH32X035G8U6 runs the dongle,
HUSB311 (TCPCI) owns the target CC for the Apple DFU VDMs.

BOM-row discipline: resistors are only 5.1k / 10k / 5.6k, caps only 100nF / 1uF
plus 220pF on the target CC lines.
"""
import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "dongle-lite", "gen"))
import gen
R={"5.1k":"0402WGF5101TCE","10k":"0402WGF1002TCE","5.6k":"0402WGF5601TCE"}
C={"100nF":"CL05B104KO5NNNC","1uF":"CL05A105KA5NQNC","220pF":"0402B221K500NT"}
XTAL="X322512MSB4SI"; LED="KT-0603R"; BTN="TS3315A"
USBC="HXTYPE-C24PQCHT"; ESD="H5VU25U"; MUX="CH9441P"; FET="HL2302"
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
#   PB7  TGT_VBUS_EN (10k pulldown, MT9700 EN must not float)
#   PB6  SBU_PU (drive high to enable the SBU level shifters, Hi-Z/low = off)
#   PA1  LED_STAT
#   SBU1_MCU = PA2 (USART2 TX) + PB4 (USART3 RX)
#   SBU2_MCU = PA3 (USART2 RX) + PB3 (USART3 TX)
#        normal cable: USART2 (TX on SBU1, RX on SBU2); flipped: USART3. The
#        idle USART's TX pin stays an input; enable the internal pull-up on
#        whichever pin is receiving. PA4..PB0 between them stay unused so the
#        two SBU nets can wrap the east side of the package.
#   PC16/PC17 USB D-/D+   PC18/PC19 SDI debug (TP9/TP10)
#   PC17 high at power-on = ROM USB bootloader
add("U1","CH32X035G8U6",(150,150,0),{
 "2":"+3V3","29":"GND",
 "21":"I2C_SDA","20":"I2C_SCL","19":"PD_INT","18":"TGT_VBUS_EN","17":"SBU_PU",
 "6":"LED_STAT",
 "7":"SBU1_MCU","8":"SBU2_MCU","14":"SBU2_MCU","15":"SBU1_MCU",
 "24":"SWCLK","25":"SWDIO","26":"MCU_D_N","27":"MCU_D_P",
 "1":None,"3":None,"4":None,"5":None,"9":None,"10":None,"11":None,"12":None,"13":None,
 "16":None,"22":None,"23":None,"28":None},"CH32X035G8U6")
c("C1","100nF",(95,230,0),"+3V3","GND"); c("C2","1uF",(120,230,0),"+3V3","GND")
# BOOT: hold SW1 while plugging in -> PC17 (D+) high at power-on -> ROM USB ISP
add("SW1",BTN,(120,78,0),{"1":"BOOT_SW","2":"BOOT_SW","3":"+3V3","4":"+3V3"})
r("R1","5.1k",(160,78,0),"BOOT_SW","MCU_D_P")
r("R2","10k",(200,110,0),"TGT_VBUS_EN","GND")

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
 "13":"TGT_CC1","14":"TGT_CC2","8":"+3V3","7":"+3V3","17":"GND",
 "1":"TGT_TX1_P","2":"TGT_TX1_N","15":"TGT_RX1_P","16":"TGT_RX1_N",
 "3":"TGT_RX2_N","4":"TGT_RX2_P","5":"TGT_TX2_N","6":"TGT_TX2_P",
 "12":"SSA_P","11":"SSA_N","10":"SSB_P","9":"SSB_N"})
c("C10","100nF",(830,230,0),"+3V3","GND")
add("J2",USBC,(950,150,0),{
 "A1":"GND","A12":"GND","B1":"GND","B12":"GND","25":"GND",
 # VBUS on the left pair only (A4/B9); the plug buses all four VBUS contacts.
 # A9/B4 sit boxed in between the D+/D- and SBU escapes, so they stay open.
 "A4":"TGT_VBUS","B9":"TGT_VBUS","A9":None,"B4":None,
 "A5":"TGT_CC1","B5":"TGT_CC2",
 "A6":"TGT_D_P","A7":"TGT_D_N","B6":"TGT_D_P","B7":"TGT_D_N",
 "A8":"TGT_SBU1","B8":"TGT_SBU2",
 "A2":"TGT_TX1_P","A3":"TGT_TX1_N","B11":"TGT_RX1_P","B10":"TGT_RX1_N",
 "B2":"TGT_TX2_P","B3":"TGT_TX2_N","A11":"TGT_RX2_P","A10":"TGT_RX2_N"})
esd("D11",(800,300,0),None,None,"TGT_D_N","TGT_D_P")
esd("D14",(870,300,0),"TGT_RX1_P","TGT_RX1_N","TGT_TX1_P","TGT_TX1_N")
esd("D15",(940,300,0),"TGT_RX2_N","TGT_RX2_P","TGT_TX2_N","TGT_TX2_P")

# ===== SBU serial cluster — 1.2 V <-> 3.3 V, one common-gate FET per line =====
# SBU_PU high: 10k/5.6k holds each SBU line at 1.18 V idle and the gate sits at
# 1.65 V, so the FET conducts only when either side pulls low and can never
# lift SBU above ~1.1 V. SBU_PU low or Hi-Z: gate at 0 V, FETs off, and each SBU
# line is left with 5.6k (low) or 15.6k (Hi-Z) to ground. The MCU side idles
# high on the MCU's internal pull-ups.
add("Q1",FET,(360,430,0),{"1":"SBU_VG","2":"TGT_SBU1","3":"SBU1_MCU"})
add("Q2",FET,(440,430,0),{"1":"SBU_VG","2":"TGT_SBU2","3":"SBU2_MCU"})
r("R8","10k",(330,470,0),"SBU_PU","TGT_SBU1"); r("R9","5.6k",(355,470,0),"TGT_SBU1","GND")
r("R10","10k",(410,470,0),"SBU_PU","TGT_SBU2"); r("R11","5.6k",(435,470,0),"TGT_SBU2","GND")
r("R12","10k",(330,500,0),"SBU_PU","SBU_VG"); r("R13","10k",(355,500,0),"SBU_VG","GND")
c("C12","100nF",(380,500,0),"SBU_VG","GND")

# ===== Power cluster =====
add("U6","ME6211C33M5G-N",(560,430,0),{"1":"+5V","2":"GND","3":"+5V","4":None,"5":"+3V3"})
c("C13","1uF",(530,470,0),"+5V","GND"); c("C14","1uF",(560,470,0),"+3V3","GND")
# Target VBUS: MT9700, Ilim = 6.8/Rset(k) = 0.68 A with 10k; EN active-high
add("U7","MT9700-N",(660,430,0),{"5":"+5V","2":"GND","4":"TGT_VBUS_EN","3":"ILIM_SET","1":"TGT_VBUS"})
r("R16","10k",(660,470,0),"ILIM_SET","GND")
c("C15","1uF",(630,470,0),"+5V","GND"); c("C16","1uF",(700,470,0),"TGT_VBUS","GND")
r("R19","10k",(700,500,0),"TGT_VBUS","GND")   # bleeder: the switch does not discharge its output
add("D1",LED,(530,510,0),{"1":"LEDP_A","2":"GND"}); r("R17","5.1k",(560,510,0),"+3V3","LEDP_A")
add("D2",LED,(600,510,0),{"1":"LEDS_A","2":"GND"}); r("R18","5.1k",(630,510,0),"LED_STAT","LEDS_A")
for i,rail in enumerate(["+5V","+3V3","GND","HUB_3V3","TGT_VBUS","HMUX_VDD"]):
    add(f"#FLG{i+1}","PWR_FLAG",(520+i*22,550,0),{"1":rail},"PWR_FLAG")

# ===== Debug / bring-up cluster =====
# SDI debug (WCH-LinkE: SWDIO, SWCLK, 3V3, GND) three ways: pads next to the MCU,
# a row of four through-holes at 2.54 mm for a pin header (not fitted), and a
# TC2030 Tag-Connect footprint with the same pinout as the other dongles
# (1 VTREF, 2 SWDIO, 4 SWCLK, 5 GND; 3 and 6 are open, this MCU has no reset pin).
comps.append({"ref":"J4","sym":"TC2030","at":(900,480,0),"in_bom":False,"value":"TC2030-IDC-NL",
              "nets":{"1":"+3V3","2":"SWDIO","3":None,"4":"SWCLK","5":"GND","6":None}})
for i,(ref,net) in enumerate([("TP11","SWCLK"),("TP12","SWDIO"),("TP13","+3V3"),("TP14","GND")]):
    comps.append({"ref":ref,"sym":"TPTH","at":(900+i*20,540,0),"in_bom":False,"value":net,"nets":{"1":net}})
# probe pads on the back for the target CC and SBU lines (PD sniffing, serial)
for i,(ref,net) in enumerate([("TP20","TGT_CC1"),("TP21","TGT_CC2"),("TP22","TGT_SBU1"),("TP23","TGT_SBU2")]):
    comps.append({"ref":ref,"sym":"TP","at":(760+i*20,540,0),"in_bom":False,"value":net,"nets":{"1":net}})
for i,(ref,net) in enumerate([("TP3","+5V"),("TP4","TGT_VBUS"),
                              ("TP9","SWCLK"),("TP10","SWDIO")]):
    comps.append({"ref":ref,"sym":"TP","at":(760+(i%4)*20,480+(i//4)*25,0),
                  "in_bom":False,"value":net,"nets":{"1":net}})

SS_PAIRS={f"{p}_{s}" for p in
  ("SSA","SSB","HOST_TX1","HOST_TX2","HOST_RX1","HOST_RX2",
   "TGT_TX1","TGT_TX2","TGT_RX1","TGT_RX2")
  for s in ("P","N")}
gen.build_wired(comps,"dongle-pro-v2.kicad_sch",
                {"GND":"GND","+3V3":"+3V3","+5V":"+5V"},
                wires=[], auto_wire_dist=90, paper="A0", project="dongle-pro-v2",
                force_label=SS_PAIRS|{"HOST_D_P","HOST_D_N","TGT_D_P","TGT_D_N","MCU_D_P","MCU_D_N",
                                      "HOST_CC1","HOST_CC2","TGT_CC1","TGT_CC2","TGT_SBU1","TGT_SBU2",
                                      "I2C_SCL","I2C_SDA","PD_INT","SBU1_MCU","SBU2_MCU","SBU_PU","SBU_VG","SWCLK","SWDIO","TGT_VBUS_EN","LED_STAT","BOOT_SW","LEDP_A","LEDS_A","ILIM_SET","HUB_XI","HUB_XO","HMUX_VDD","HUB_3V3"})
print(f"{len(comps)} comps")
