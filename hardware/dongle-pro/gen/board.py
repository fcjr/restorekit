#!/usr/bin/env python3
"""Dongle-Lite-USB3 -> single-sheet wired KiCad schematic, in functional clusters.
USB 3.1 Gen 1 (5 Gbps) variant: GL3510 USB3 hub in the SS path, HD3SS3220 host
CC controller + SS mux, HD3SS3212 target SS mux (FUSB302B keeps sole ownership
of target CC for the Apple DFU VDM). Local nets are wired; rails use power
ports; cross-cluster signals and all diff pairs use labels."""
import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "dongle-lite", "gen"))
import gen
R={"5.1k":"0402WGF5101TCE","4.7k":"0402WGF4701TCE","1k":"0402WGF1001TCE","10k":"0402WGF1002TCE",
   "47k":"0603WAF4702T5E","33":"0402WGF330JTCE","20k":"0402WGF2002TCE","100k":"0402WGF1003TCE",
   "200k":"0402WGF2003TCE","910k":"0402WGF9103TCE"}
C={"100nF":"CL05B104KO5NNNC","1uF":"CL05A105KA5NQNC","22pF":"CL21C220JBANNNC","10uF":"CL21A106KAYNNNE","4.7uF":"CL05A475KP5NRNC"}
XTAL="X322512MSB4SI"; XTAL25="X322525MOB4SI"; LED="KT-0603R"; BTN="TS-1187A-B-A-B"
comps=[]
def add(ref,sym,at,nets,value=None): comps.append({"ref":ref,"sym":sym,"at":at,"nets":nets,"value":value})
def r(ref,val,at,a,b): add(ref,R[val],at,{"1":a,"2":b},val)
def c(ref,val,at,a,b): add(ref,C[val],at,{"1":a,"2":b},val)

# ===== MCU core cluster — RP2354A (RP2350 die + 2MB internal flash) =====
# GPIO map for firmware:  GP16=I2C_SDA GP17=I2C_SCL GP18=FUSB_INT GP19=TGT_VBUS_EN
#   GP20=SBU1_DIR GP21=SBU2_DIR GP22=SBU1_UART GP23=SBU2_UART GP24=SHIFT_EN GP25=LED_STAT
#   GP26=SS_SEL (HD3SS3212 lane select, H=CC2/flipped)  GP27=HUB_RSTn (GL3510 reset, drive low to reset)
add("U1","RP2354A_C41378174",(150,150,0),{
 # IOVDD (3V3 IO) x6
 "1":"+3V3","11":"+3V3","20":"+3V3","30":"+3V3","38":"+3V3","45":"+3V3",
 # DVDD (1.1V core) x3 — from on-chip SMPS
 "6":"+1V1","23":"+1V1","39":"+1V1",
 # analog / QSPI / OTP supplies -> 3V3 (ADC unused)
 "44":"+3V3","54":"+3V3","53":"+3V3",
 # on-chip switching regulator (RP2350 HW design guide)
 "49":"+3V3","48":"VREG_LX","50":"+1V1","47":"GND","46":"VREG_AVDD",
 # clock / reset / debug
 "21":"XIN","22":"XOUT","24":"SWCLK","25":"SWDIO","26":"RUN",
 # USB
 "52":"MCU_D_P","51":"MCU_D_N",
 # QSPI: internal flash. Keep SS for BOOTSEL; SD0-3+SCLK internal (NC ext)
 "60":"QSPI_SS","55":None,"56":None,"57":None,"58":None,"59":None,
 # GPIO signals
 "27":"I2C_SDA","28":"I2C_SCL","29":"FUSB_INT","31":"TGT_VBUS_EN",
 "32":"SBU1_DIR","33":"SBU2_DIR","34":"SBU1_UART","35":"SBU2_UART","36":"SHIFT_EN","37":"LED_STAT",
 "40":"SS_SEL","41":"HUB_RSTn",
 # GND (exposed pad)
 "61":"GND",
 # unused GPIO
 "2":None,"3":None,"4":None,"5":None,"7":None,"8":None,"9":None,"10":None,
 "12":None,"13":None,"14":None,"15":None,"16":None,"17":None,"18":None,"19":None,
 "42":None,"43":None},"RP2354A")
# on-chip SMPS support components
add("L1","AOTA-B201610S3R3-101-T",(115,105,0),{"1":"VREG_LX","2":"+1V1"},"3.3uH")
r("R12","33",(180,120,0),"+3V3","VREG_AVDD")
c("C6","4.7uF",(95,120,0),"+3V3","GND")   # VREG_VIN input
c("C7","4.7uF",(135,90,0),"+1V1","GND")   # VREG output
c("C9","4.7uF",(205,120,0),"VREG_AVDD","GND")  # VREG_AVDD filter
# BOOTSEL: button -> R6(1k) -> QSPI_SS; pull QSPI_SS low at reset to enter USB boot
add("SW1",BTN,(120,78,0),{"1":"BOOT_SW","2":"BOOT_SW","3":"GND","4":"GND"})
r("R13","1k",(150,78,0),"QSPI_SS","BOOT_SW")
add("Y1",XTAL,(140,230,0),{"1":"XIN","3":"XOUT","2":"GND","4":"GND"})
c("C1","22pF",(115,235,0),"XIN","GND"); c("C2","22pF",(165,235,0),"XOUT","GND")
r("R1","10k",(205,210,0),"RUN","+3V3")
row=[("C4","1uF","+1V1"),("C5","100nF","+1V1"),("C8","100nF","+3V3"),
     ("C10","100nF","+3V3"),("C11","100nF","+3V3"),("C12","100nF","+3V3"),("C27","100nF","+3V3")]
for i,(ref,val,rail) in enumerate(row): c(ref,val,(95+i*20,310,0),rail,"GND")

# ===== USB-PD cluster (x~90-260, y~400-500) =====
add("U2","FUSB302BMPX",(150,430,0),{
 "3":"+3V3","4":"+3V3","8":"GND","9":"GND","15":"GND",
 "10":"TGT_CC1","11":"TGT_CC1","1":"TGT_CC2","14":"TGT_CC2","12":None,"13":None,"2":None,
 "5":"FUSB_INT","6":"I2C_SCL","7":"I2C_SDA"})
r("R2","4.7k",(100,470,0),"I2C_SDA","+3V3"); r("R3","4.7k",(130,470,0),"I2C_SCL","+3V3"); r("R4","4.7k",(160,470,0),"FUSB_INT","+3V3")
c("C13","100nF",(200,470,0),"+3V3","GND"); c("C14","1uF",(230,470,0),"+3V3","GND")

# ===== Host port + CC controller cluster (x~340-580, y~80-320) =====
# HD3SS3220 in UFP-only mode (PORT=GND) owns host CC: internal Rd replaces the
# old discrete 5.1k pulldowns. Autonomous — no I2C (ADDR NC = GPIO mode).
add("J1","TYPE-C24PQT",(360,150,0),{
 "A1":"GND","A12":"GND","B1":"GND","B12":"GND","3":"GND","4":"GND","25":"GND",
 "A4":"+5V","A9":"+5V","B4":"+5V","B9":"+5V",
 "A5":"HOST_CC1","B5":"HOST_CC2",
 "A6":"HOST_D_P","A7":"HOST_D_N","B6":"HOST_D_P","B7":"HOST_D_N",
 "A8":None,"B8":None,
 "A2":"HOST_TX1_P","A3":"HOST_TX1_N","B11":"HOST_RX1_P","B10":"HOST_RX1_N",
 "B2":"HOST_TX2_P","B3":"HOST_TX2_N","A11":"HOST_RX2_P","A10":"HOST_RX2_N"})
add("U3","HD3SS3220RNHR",(470,170,0),{
 "1":"HOST_CC2","2":"HOST_CC1","3":None,"4":"GND","5":"VBUS_DET",
 "6":"HUB_UTX_P","7":"HUB_UTX_N","9":"HUB_URX_P","10":"HUB_URX_N",
 "8":"+3V3","30":"+5V","13":"GND","28":"GND","31":"GND",
 "11":"MUX_DIR","12":"GND","29":"GND",
 "14":"HOST_RX1_N","15":"HOST_RX1_P","16":"M1_TX1_N","17":"M1_TX1_P",
 "18":"HOST_RX2_N","19":"HOST_RX2_P","20":"M1_TX2_N","21":"M1_TX2_P",
 "22":None,"23":None,"24":None,"25":None,"26":None,"27":None})
# SS TX AC coupling (one set per link segment, on the transmitter side): 100nF 0402
c("C31","100nF",(415,110,0),"M1_TX1_P","HOST_TX1_P"); c("C32","100nF",(415,125,0),"M1_TX1_N","HOST_TX1_N")
c("C33","100nF",(415,140,0),"M1_TX2_P","HOST_TX2_P"); c("C34","100nF",(415,155,0),"M1_TX2_N","HOST_TX2_N")
r("R14","910k",(530,110,0),"+5V","VBUS_DET")      # VBUS_DET: 880-920k per HD3SS3220 DS
r("R15","200k",(530,125,0),"MUX_DIR","+3V3")      # DIR pull-up (mandatory per DS); test point below
c("C28","100nF",(530,230,0),"+5V","GND"); c("C29","1uF",(555,230,0),"+5V","GND")   # VDD5
c("C30","100nF",(505,230,0),"+3V3","GND")                                          # VCC33
# ESD: USBLC6 on D+/D- (as before), flow-through TPD4E05U06 per SS lane
add("D10","USBLC6-2SC6",(360,280,0),{"1":"HOST_D_N","6":"HOST_D_N","3":"HOST_D_P","4":"HOST_D_P","2":"GND","5":"+5V"})
add("D12","TPD4E05U06DQAR_C138714",(420,280,0),{
 "1":"HOST_TX1_N","10":"HOST_TX1_N","2":"HOST_TX1_P","9":"HOST_TX1_P",
 "4":"HOST_RX1_N","7":"HOST_RX1_N","5":"HOST_RX1_P","6":"HOST_RX1_P","3":"GND","8":"GND"})
add("D13","TPD4E05U06DQAR_C138714",(480,280,0),{
 "1":"HOST_TX2_P","10":"HOST_TX2_P","2":"HOST_TX2_N","9":"HOST_TX2_N",
 "4":"HOST_RX2_P","7":"HOST_RX2_P","5":"HOST_RX2_N","6":"HOST_RX2_N","3":"GND","8":"GND"})

# ===== USB3 hub cluster (x~640-900, y~80-340) — GL3510-OSY52, 4-DFP die =====
# DS1 = MCU (USB2 only), DS2 = target (SS+USB2), DS3 unused (NC), DS4 strap-disabled (FN_B).
add("U4","GL3510-OSY52",(770,180,0),{
 # upstream port 0
 "36":"HOST_D_N","37":"HOST_D_P",
 "38":"HUB_UTX_N","39":"HUB_UTX_P","41":"HUB_URX_N","42":"HUB_URX_P",
 # DS1: MCU, USB2 only
 "52":"MCU_D_P","53":"MCU_D_N","47":None,"48":None,"50":None,"51":None,
 # DS2: target, SS+USB2
 "56":"HUB_DTX_N","57":"HUB_DTX_P","59":"HUB_DRX_N","60":"HUB_DRX_P",
 "63":"TGT_D_P","64":"TGT_D_N",
 # DS3 unused, DS4 disabled
 "1":None,"2":None,"4":None,"5":None,"6":None,"7":None,
 "9":None,"10":None,"11":None,"12":None,"14":None,"15":None,
 # clock / bias
 "45":"HUB_XI","44":"HUB_XO","16":"HUB_RTERM",
 # power: 5V in, 3.3V LDO out, 1.2V buck (SW->L2->HUB_1V2, FB sense)
 "20":"+5V","21":"HUB_3V3","19":"HUB_SW","17":"HUB_1V2","18":"GND",
 "26":"HUB_3V3","62":"HUB_3V3","8":"HUB_3V3","35":"HUB_3V3","43":"HUB_3V3","54":"HUB_3V3",
 "27":"HUB_1V2","55":"HUB_1V2","61":"HUB_1V2",
 "40":"HUB_1V2","49":"HUB_1V2","58":"HUB_1V2","3":"HUB_1V2","13":"HUB_1V2",
 "46":"HUB_AV12",
 # control / straps
 "24":"HUB_RSTn","25":"HUB_VBUS_SNS",
 "22":None,"23":"HUB_FNB","32":"HUB_FNC","33":None,"34":None,
 "28":None,"29":None,"30":None,"31":None,
 "65":"GND"})
add("Y2",XTAL25,(700,290,0),{"1":"HUB_XI","3":"HUB_XO","2":"GND","4":"GND"},"25MHz")
c("C15","22pF",(675,295,0),"HUB_XI","GND"); c("C16","22pF",(725,295,0),"HUB_XO","GND")
r("R16","20k",(660,110,0),"HUB_RTERM","GND")      # RTERM bias, 1%, mandatory
# RESETJ: VBUS-sense divider (~3.4V) + 1uF delay; MCU GP27 can yank it low
r("R17","47k",(660,140,0),"+5V","HUB_RSTn"); r("R18","100k",(660,155,0),"HUB_RSTn","GND")
c("C37","1uF",(660,170,0),"HUB_RSTn","GND")
# VBUS-valid sense divider
r("R19","47k",(660,200,0),"+5V","HUB_VBUS_SNS"); r("R20","100k",(660,215,0),"HUB_VBUS_SNS","GND")
# straps: FN_B=1 disable DS4; PLED/FN_C=1 disable BC1.2 charging
r("R21","10k",(660,245,0),"HUB_FNB","GND"); r("R22","10k",(660,260,0),"HUB_FNC","GND")
# 1.2V buck: SW -> 3.3uH -> HUB_1V2 (FB tied to rail; fixed-voltage version)
add("L2","AOTA-B201610S3R3-101-T",(860,110,0),{"1":"HUB_SW","2":"HUB_1V2"},"3.3uH")
c("C38","10uF",(840,290,0),"+5V","GND"); c("C39","100nF",(865,290,0),"+5V","GND")
c("C40","10uF",(640,320,0),"HUB_3V3","GND")
for i in range(4): c(f"C{41+i}","100nF",(665+i*20,320,0),"HUB_3V3","GND")
c("C45","10uF",(755,320,0),"HUB_1V2","GND"); c("C46","10uF",(780,320,0),"HUB_1V2","GND")
for i in range(4): c(f"C{47+i}","100nF",(805+i*20,320,0),"HUB_1V2","GND")
# AVDD12 (PLL) fed from HUB_1V2 with local decoupling, keep quiet
c("C51","1uF",(890,140,0),"HUB_AV12","HUB_1V2"); c("C52","100nF",(890,155,0),"HUB_AV12","GND")

# ===== Target SS mux + port cluster (x~960-1160, y~80-320) =====
# HD3SS3212 is data-only: FUSB302B exclusively owns TGT_CC1/2 (Apple DFU VDM).
# Channel wiring (layout-driven): B = J2 lane 2 (flipped/CC2), C = J2 lane 1
# (normal/CC1). Chip: SEL=L -> A<->B, SEL=H -> A<->C, so firmware drives
# SEL = HIGH for CC1/normal orientation, LOW for CC2/flipped.
add("U7","HD3SS3212IRKSR",(1000,170,0),{
 "1":None,"10":None,
 "2":"MUX_OE","9":"SS_SEL",
 "3":"M2_A0_P","4":"M2_A0_N","7":"HUB_DRX_P","8":"HUB_DRX_N",
 "19":"TGT_TX2_P","18":"TGT_TX2_N","17":"TGT_RX2_P","16":"TGT_RX2_N",
 "15":"TGT_TX1_P","14":"TGT_TX1_N","13":"TGT_RX1_P","12":"TGT_RX1_N",
 "6":"+3V3","5":"GND","11":"GND","20":"GND","21":"GND"})
# DS2 TX AC coupling (hub-side of the mux per TI Fig 8)
c("C35","100nF",(955,110,0),"HUB_DTX_P","M2_A0_P"); c("C36","100nF",(955,125,0),"HUB_DTX_N","M2_A0_N")
r("R23","10k",(1000,255,0),"SS_SEL","GND")        # lane-1 default before firmware runs
r("R24","10k",(1030,255,0),"MUX_OE","GND")        # always enabled
c("C53","10uF",(1060,255,0),"+3V3","GND"); c("C54","1uF",(1085,255,0),"+3V3","GND"); c("C55","100nF",(1110,255,0),"+3V3","GND")
add("J2","TYPE-C24PQT",(1120,150,0),{
 "A1":"GND","A12":"GND","B1":"GND","B12":"GND","3":"GND","4":"GND","25":"GND",
 "A4":"TGT_VBUS","A9":"TGT_VBUS","B4":"TGT_VBUS","B9":"TGT_VBUS",
 "A5":"TGT_CC1","B5":"TGT_CC2",
 "A6":"TGT_D_P","A7":"TGT_D_N","B6":"TGT_D_P","B7":"TGT_D_N",
 "A8":"TGT_SBU1","B8":"TGT_SBU2",
 "A2":"TGT_TX1_P","A3":"TGT_TX1_N","B11":"TGT_RX1_P","B10":"TGT_RX1_N",
 "B2":"TGT_TX2_P","B3":"TGT_TX2_N","A11":"TGT_RX2_P","A10":"TGT_RX2_N"})
add("D11","USBLC6-2SC6",(1000,310,0),{"1":"TGT_D_P","6":"TGT_D_P","3":"TGT_D_N","4":"TGT_D_N","2":"GND","5":"TGT_VBUS"})
add("D14","TPD4E05U06DQAR_C138714",(1060,310,0),{
 "1":"TGT_RX1_N","10":"TGT_RX1_N","2":"TGT_RX1_P","9":"TGT_RX1_P",
 "4":"TGT_TX1_N","7":"TGT_TX1_N","5":"TGT_TX1_P","6":"TGT_TX1_P","3":"GND","8":"GND"})
add("D15","TPD4E05U06DQAR_C138714",(1120,310,0),{
 "1":"TGT_RX2_N","10":"TGT_RX2_N","2":"TGT_RX2_P","9":"TGT_RX2_P",
 "4":"TGT_TX2_N","7":"TGT_TX2_N","5":"TGT_TX2_P","6":"TGT_TX2_P","3":"GND","8":"GND"})

# ===== SBU serial cluster (x~340-460, y~400-490) =====
add("U8","74AVC1T45GW,125",(360,430,0),{"1":"+3V3","2":"GND","3":"SBU1_UART","4":"TGT_SBU1","5":"SBU1_DIR","6":"+1V2"})
add("U9","74AVC1T45GW,125",(430,430,0),{"1":"+3V3","2":"GND","3":"SBU2_UART","4":"TGT_SBU2","5":"SBU2_DIR","6":"+1V2"})
c("C20","100nF",(390,470,0),"+3V3","GND"); c("C21","100nF",(450,470,0),"+1V2","GND")

# ===== Power cluster (x~500-700, y~400-520) =====
# RT9013 EN gets an RC delay so +5V (HD3SS3220 VDD5) is stable >=2ms before +3V3
# (VCC33) comes up — t_VDD5V_PG power sequencing per HD3SS3220 DS 6.3.12.
add("U5","RT9013-33GB",(520,430,0),{"1":"+5V","2":"GND","3":"LDO_EN","4":None,"5":"+3V3"})
r("R25","100k",(490,405,0),"+5V","LDO_EN"); c("C56","100nF",(490,420,0),"LDO_EN","GND")
add("U6","TLV70212DBVR",(590,430,0),{"1":"+3V3","2":"GND","3":"SHIFT_EN","4":None,"5":"+1V2"})
add("U10","AP22653W6-7",(660,430,0),{"1":"+5V","2":"GND","3":"TGT_VBUS_EN","4":"VBUS_FAULT","5":"ILIM_SET","6":"TGT_VBUS"})
r("R8","47k",(660,470,0),"ILIM_SET","GND"); r("R9","10k",(690,470,0),"VBUS_FAULT","+3V3")
add("D1",LED,(510,480,0),{"1":"LEDP_A","2":"GND"}); r("R10","1k",(540,480,0),"+3V3","LEDP_A")
add("D2",LED,(570,480,0),{"1":"LEDS_A","2":"GND"}); r("R11","1k",(600,480,0),"LED_STAT","LEDS_A")
c("C22","10uF",(520,470,0),"+5V","GND"); c("C23","1uF",(550,470,0),"+3V3","GND"); c("C24","1uF",(590,470,0),"+1V2","GND")
c("C25","100nF",(630,470,0),"+5V","GND"); c("C26","1uF",(700,430,0),"TGT_VBUS","GND")
for i,rail in enumerate(["+5V","+3V3","+1V2","+1V1","GND"]):
    add(f"#FLG{i+1}","PWR_FLAG",(500+i*22,520,0),{"1":rail},"PWR_FLAG")

# ===== Debug / bring-up cluster (x~760-900, y~400-520) =====
comps.append({"ref":"J3","sym":"TC2030","at":(790,430,0),"in_bom":False,"value":"TC2030-IDC-NL",
              "nets":{"1":"+3V3","2":"SWDIO","3":"RUN","4":"SWCLK","5":"GND","6":None}})
for i,net in enumerate(["+5V","+3V3","+1V2","+1V1","TGT_VBUS","GND","RUN","FUSB_INT",
                        "HUB_3V3","HUB_1V2","HUB_RSTn"]):
    comps.append({"ref":f"TP{i+1}","sym":"TP","at":(760+(i%4)*20,470+(i//4)*25,0),
                  "in_bom":False,"value":net,"nets":{"1":net}})
add("#FLG6","PWR_FLAG",(620,320,0),{"1":"HUB_3V3"},"PWR_FLAG")
add("#FLG7","PWR_FLAG",(700,470,0),{"1":"TGT_VBUS"},"PWR_FLAG")
add("#FLG8","PWR_FLAG",(735,320,0),{"1":"HUB_1V2"},"PWR_FLAG")

SS_PAIRS={f"{p}_{s}" for p in
  ("HUB_UTX","HUB_URX","HUB_DTX","HUB_DRX","M2_A0",
   "M1_TX1","M1_TX2","HOST_TX1","HOST_TX2","HOST_RX1","HOST_RX2",
   "TGT_TX1","TGT_TX2","TGT_RX1","TGT_RX2")
  for s in ("P","N")}
gen.build_wired(comps,"dongle-pro.kicad_sch",
                {"GND":"GND","+3V3":"+3V3","+1V2":"+1V2","+5V":"+5V","+1V1":"+1V1"},
                wires=[], auto_wire_dist=90, paper="A0", project="dongle-pro",
                force_label=SS_PAIRS|{"HOST_D_P","HOST_D_N","TGT_D_P","TGT_D_N","MCU_D_P","MCU_D_N"})
print(f"{len(comps)} comps")
