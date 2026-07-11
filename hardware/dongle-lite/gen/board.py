#!/usr/bin/env python3
"""Dongle-Proto-Lite -> single-sheet wired KiCad schematic, in functional clusters.
Local nets are wired; rails use power ports; cross-cluster signals use labels."""
import gen
R={"5.1k":"0402WGF5101TCE","4.7k":"0402WGF4701TCE","1k":"0402WGF1001TCE","10k":"0402WGF1002TCE","47k":"0603WAF4702T5E"}
C={"100nF":"CL05B104KO5NNNC","1uF":"CL05A105KA5NQNC","22pF":"CL21C220JBANNNC","10uF":"CL21A106KAYNNNE"}
XTAL="X322512MSB4SI"; LED="KT-0603R"; BTN="TS-1187A-B-A-B"; MCU="RP2040"
comps=[]
def add(ref,sym,at,nets,value=None): comps.append({"ref":ref,"sym":sym,"at":at,"nets":nets,"value":value})
def r(ref,val,at,a,b): add(ref,R[val],at,{"1":a,"2":b},val)
def c(ref,val,at,a,b): add(ref,C[val],at,{"1":a,"2":b},val)

# ===== MCU core cluster (x~90-260, y~60-320) =====
add("U1",MCU,(150,150,0),{
 "1":"+3V3","10":"+3V3","22":"+3V3","33":"+3V3","42":"+3V3","49":"+3V3",
 "48":"+3V3","43":"+3V3","44":"+3V3",
 "45":"+1V1","23":"+1V1","50":"+1V1","57":"GND","19":"GND",
 "20":"XIN","21":"XOUT","24":"SWCLK","25":"SWDIO","26":"RUN",
 "56":"QSPI_SS","52":"QSPI_SCLK","53":"QSPI_SD0","55":"QSPI_SD1","54":"QSPI_SD2","51":"QSPI_SD3",
 "47":"MCU_DP","46":"MCU_DM",
 "13":"SBU1_DIR","14":"SBU2_DIR","15":"SBU1_UART","16":"SBU2_UART","17":"SHIFT_EN",
 "27":"I2C_SDA","28":"I2C_SCL","30":"TGT_VBUS_EN","31":"FUSB_INT","37":"LED_STAT",
 "2":None,"3":None,"4":None,"5":None,"6":None,"7":None,"8":None,"9":None,"11":None,"12":None,
 "18":None,"29":None,"32":None,"34":None,"35":None,"36":None,
 "38":None,"39":None,"40":None,"41":None},"RP2040")
add("U4","W25Q32JVSSIQ_C2834491",(150,75,0),{"1":"QSPI_SS","6":"QSPI_SCLK","5":"QSPI_SD0","2":"QSPI_SD1","3":"QSPI_SD2","7":"QSPI_SD3","8":"+3V3","4":"GND"})
add("SW1",BTN,(120,78,0),{"1":"QSPI_SS","2":"QSPI_SS","3":"GND","4":"GND"})
add("Y1",XTAL,(140,230,0),{"1":"XIN","3":"XOUT","2":"GND","4":"GND"})
c("C1","22pF",(115,235,0),"XIN","GND"); c("C2","22pF",(165,235,0),"XOUT","GND")
r("R1","10k",(205,210,0),"RUN","+3V3")
row=[("C4","1uF","+1V1"),("C5","100nF","+1V1"),("C6","1uF","+3V3"),("C7","100nF","+3V3"),
     ("C8","100nF","+3V3"),("C9","100nF","+3V3"),("C10","100nF","+3V3"),("C11","100nF","+3V3"),("C12","100nF","+3V3")]
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
 "15":"HOST_DP","14":"HOST_DM","12":"MCU_DP","11":"MCU_DM","10":"TGT_DP","9":"TGT_DM",
 "8":None,"7":None,"6":None,"5":None,"4":"HUB_XI","3":"HUB_XO","18":"GND","16":"HUB_RSTB","1":"HUB_3V3",
 "24":None,"23":None,"22":None,"21":None,"13":None,"17":None,"2":None})
add("J1","TYPE-C-31-M-12",(370,150,0),{
 "A1B12":"GND","B1A12":"GND","1":"GND","2":"GND","3":"GND","4":"GND","A4B9":"+5V","B4A9":"+5V",
 "A5":"HOST_CC1","B5":"HOST_CC2","A6":"HOST_DP","A7":"HOST_DM","B6":"HOST_DP","B7":"HOST_DM","A8":None,"B8":None})
add("J2","TYPE-C-31-M-12",(560,150,0),{
 "A1B12":"GND","B1A12":"GND","1":"GND","2":"GND","3":"GND","4":"GND","A4B9":"TGT_VBUS","B4A9":"TGT_VBUS",
 "A5":"TGT_CC1","B5":"TGT_CC2","A6":"TGT_DP","A7":"TGT_DM","B6":"TGT_DP","B7":"TGT_DM","A8":"TGT_SBU1","B8":"TGT_SBU2"})
add("D10","USBLC6-2SC6",(410,95,0),{"1":"HOST_DP","6":"HOST_DP","3":"HOST_DM","4":"HOST_DM","2":"GND","5":"+5V"})
add("D11","USBLC6-2SC6",(510,95,0),{"1":"TGT_DP","6":"TGT_DP","3":"TGT_DM","4":"TGT_DM","2":"GND","5":"TGT_VBUS"})
add("Y2",XTAL,(455,235,0),{"1":"HUB_XI","3":"HUB_XO","2":"GND","4":"GND"})
c("C15","22pF",(430,240,0),"HUB_XI","GND"); c("C16","22pF",(480,240,0),"HUB_XO","GND")
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
add("#FLG6","PWR_FLAG",(620,235,0),{"1":"HUB_3V3"},"PWR_FLAG")
add("#FLG7","PWR_FLAG",(700,470,0),{"1":"TGT_VBUS"},"PWR_FLAG")

gen.build_wired(comps,"dongle-lite.kicad_sch",
                {"GND":"GND","+3V3":"+3V3","+1V2":"+1V2","+5V":"+5V","+1V1":"+1V1"},
                wires=[], auto_wire_dist=90, paper="A1",
                force_label={"HOST_DP","HOST_DM","TGT_DP","TGT_DM"})
print(f"{len(comps)} comps")
