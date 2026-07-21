#!/usr/bin/env python3
"""Dongle-Proto-Lite -> single-sheet wired KiCad schematic, in functional clusters.
Local nets are wired; rails use power ports; cross-cluster signals use labels."""
import gen
R={"5.1k":"0402WGF5101TCE","4.7k":"0402WGF4701TCE","1k":"0402WGF1001TCE","10k":"0402WGF1002TCE","47k":"0603WAF4702T5E","33":"0402WGF330JTCE"}
C={"100nF":"CL05B104KO5NNNC","1uF":"CL05A105KA5NQNC","22pF":"CL21C220JBANNNC","10uF":"CL21A106KAYNNNE","4.7uF":"CL05A475KP5NRNC"}
XTAL="X322512MSB4SI"; LED="KT-0603R"; BTN="TS-1187A-B-A-B"; MCU="RP2354A"
comps=[]
def add(ref,sym,at,nets,value=None): comps.append({"ref":ref,"sym":sym,"at":at,"nets":nets,"value":value})
def r(ref,val,at,a,b): add(ref,R[val],at,{"1":a,"2":b},val)
def c(ref,val,at,a,b): add(ref,C[val],at,{"1":a,"2":b},val)

# ===== MCU core cluster — RP2354A (RP2350 die + 2MB internal flash) =====
# GPIO map for firmware:  GP16=I2C_SDA GP17=I2C_SCL GP18=FUSB_INT GP19=TGT_VBUS_EN
#   GP20=SBU1_DIR GP21=SBU2_DIR GP22=SBU1_UART GP23=SBU2_UART GP24=SHIFT_EN GP25=LED_STAT
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
 # GND (exposed pad)
 "61":"GND",
 # unused GPIO
 "2":None,"3":None,"4":None,"5":None,"7":None,"8":None,"9":None,"10":None,
 "12":None,"13":None,"14":None,"15":None,"16":None,"17":None,"18":None,"19":None,
 "40":None,"41":None,"42":None,"43":None},"RP2354A")
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

# ===== USB hub + ports cluster (x~360-620, y~80-300) =====
add("U3","CH334F_C5187527",(460,150,0),{
 "19":"+5V","20":"HUB_3V3","25":"GND",
 "15":"HOST_D_P","14":"HOST_D_N","12":"MCU_D_P","11":"MCU_D_N","10":"TGT_D_P","9":"TGT_D_N",
 "8":None,"7":None,"6":None,"5":None,"4":"HUB_XI","3":"HUB_XO","18":"GND","16":"HUB_RSTB","1":"HUB_3V3",
 "24":None,"23":None,"22":None,"21":None,"13":None,"17":None,"2":None})
add("J1","TYPE-C-31-M-12",(370,150,0),{
 "A1B12":"GND","B1A12":"GND","1":"GND","2":"GND","3":"GND","4":"GND","A4B9":"+5V","B4A9":"+5V",
 "A5":"HOST_CC1","B5":"HOST_CC2","A6":"HOST_D_P","A7":"HOST_D_N","B6":"HOST_D_P","B7":"HOST_D_N","A8":None,"B8":None})
add("J2","TYPE-C-31-M-12",(560,150,0),{
 "A1B12":"GND","B1A12":"GND","1":"GND","2":"GND","3":"GND","4":"GND","A4B9":"TGT_VBUS","B4A9":"TGT_VBUS",
 "A5":"TGT_CC1","B5":"TGT_CC2","A6":"TGT_D_P","A7":"TGT_D_N","B6":"TGT_D_P","B7":"TGT_D_N","A8":"TGT_SBU1","B8":"TGT_SBU2"})
add("D10","USBLC6-2SC6",(410,95,0),{"1":"HOST_D_P","6":"HOST_D_P","3":"HOST_D_N","4":"HOST_D_N","2":"GND","5":"+5V"})
add("D11","USBLC6-2SC6",(510,95,0),{"1":"TGT_D_P","6":"TGT_D_P","3":"TGT_D_N","4":"TGT_D_N","2":"GND","5":"TGT_VBUS"})
add("Y2",XTAL,(455,235,0),{"1":"HUB_XI","3":"HUB_XO","2":"GND","4":"GND"})
c("C15","22pF",(430,240,0),"HUB_XO","GND"); c("C16","22pF",(480,240,0),"HUB_XI","GND")
r("R5","10k",(500,230,0),"HUB_RSTB","HUB_3V3")
r("R6","5.1k",(360,235,0),"HOST_CC1","GND"); r("R7","5.1k",(390,235,0),"HOST_CC2","GND")
c("C17","100nF",(520,235,0),"+5V","GND"); c("C18","1uF",(550,235,0),"HUB_3V3","GND"); c("C19","100nF",(580,235,0),"HUB_3V3","GND")

# ===== SBU serial cluster (x~340-460, y~400-490) =====
add("U8","74AVC1T45GW,125",(360,430,0),{"1":"+3V3","2":"GND","3":"SBU1_UART","4":"TGT_SBU1","5":"SBU1_DIR","6":"+1V2"})
add("U9","74AVC1T45GW,125",(430,430,0),{"1":"+3V3","2":"GND","3":"SBU2_UART","4":"TGT_SBU2","5":"SBU2_DIR","6":"+1V2"})
c("C20","100nF",(390,470,0),"+3V3","GND"); c("C21","100nF",(450,470,0),"+1V2","GND")

# ===== Power cluster (x~500-680, y~400-520) =====
add("U5","RT9013-33GB",(520,430,0),{"1":"+5V","2":"GND","3":"+5V","4":None,"5":"+3V3"})
add("U6","TLV70212DBVR",(590,430,0),{"1":"+3V3","2":"GND","3":"SHIFT_EN","4":None,"5":"+1V2"})
add("U10","AP22653W6-7",(660,430,0),{"1":"+5V","2":"GND","3":"TGT_VBUS_EN","4":"VBUS_FAULT","5":"ILIM_SET","6":"TGT_VBUS"})
r("R8","47k",(660,470,0),"ILIM_SET","GND"); r("R9","10k",(690,470,0),"VBUS_FAULT","+3V3")
add("D1",LED,(510,480,0),{"1":"LEDP_A","2":"GND"}); r("R10","1k",(540,480,0),"+3V3","LEDP_A")
add("D2",LED,(570,480,0),{"1":"LEDS_A","2":"GND"}); r("R11","1k",(600,480,0),"LED_STAT","LEDS_A")
c("C22","10uF",(520,470,0),"+5V","GND"); c("C23","1uF",(550,470,0),"+3V3","GND"); c("C24","1uF",(590,470,0),"+1V2","GND")
c("C25","100nF",(630,470,0),"+5V","GND"); c("C26","1uF",(700,430,0),"TGT_VBUS","GND")
for i,rail in enumerate(["+5V","+3V3","+1V2","+1V1","GND"]):
    add(f"#FLG{i+1}","PWR_FLAG",(500+i*22,520,0),{"1":rail},"PWR_FLAG")

# ===== Debug / bring-up cluster (x~740-810) =====
comps.append({"ref":"J3","sym":"TC2030","at":(770,150,0),"in_bom":False,"value":"TC2030-IDC-NL",
              "nets":{"1":"+3V3","2":"SWDIO","3":"RUN","4":"SWCLK","5":"GND","6":None}})
for i,net in enumerate(["+5V","+3V3","+1V2","+1V1","TGT_VBUS","GND","RUN","FUSB_INT"]):
    comps.append({"ref":f"TP{i+1}","sym":"TP","at":(740+(i%4)*20,230+(i//4)*40,0),
                  "in_bom":False,"value":net,"nets":{"1":net}})
add("#FLG6","PWR_FLAG",(620,235,0),{"1":"HUB_3V3"},"PWR_FLAG")
add("#FLG7","PWR_FLAG",(700,470,0),{"1":"TGT_VBUS"},"PWR_FLAG")

gen.build_wired(comps,"dongle-lite.kicad_sch",
                {"GND":"GND","+3V3":"+3V3","+1V2":"+1V2","+5V":"+5V","+1V1":"+1V1"},
                wires=[], auto_wire_dist=90, paper="A1",
                force_label={"HOST_D_P","HOST_D_N","TGT_D_P","TGT_D_N"})
print(f"{len(comps)} comps")
