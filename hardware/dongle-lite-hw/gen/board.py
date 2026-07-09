#!/usr/bin/env python3
"""Dongle-Proto-Lite schematic -> hierarchical KiCad 10 sheets.

RP2350A + FUSB302B (PD source) + CH334F (USB2 hub) + SBU 1.2V serial + power,
organized into 5 functional sheets. Nets used on one sheet are local labels;
shared rails/buses are global. GPIO map matches the firmware.
"""
import gen

R = {"5.1k":"0402WGF5101TCE","4.7k":"0402WGF4701TCE","1k":"0402WGF1001TCE",
     "10k":"0402WGF1002TCE","22R":"0402WGF220JTCE","47k":"0603WAF4702T5E"}
C = {"100nF":"CL05B104KO5NNNC","1uF":"CL05A105KA5NQNC","22pF":"CL21C220JBANNNC",
     "470pF":"0402B471K500NT","10uF":"CL21A106KAYNNNE"}
XTAL="X322512MSB4SI"; LED="KT-0603R"; BTN="TS-1187A-B-A-B"
IND="AOTA-B201610S3R3-101-T"; MCU="RP2350A_C42411118"

class Sheet:
    def __init__(s, name, file):
        s.name=name; s.file=file; s.comps=[]; s.i=0
    def _at(s):
        i=s.i; s.i+=1; return (50.8+(i%6)*63.5, 38.1+(i//6)*63.5, 0)
    def add(s, ref, sym, nets, value=None):
        s.comps.append({"ref":ref,"sym":sym,"at":s._at(),"nets":nets,"value":value})
    def res(s, ref, val, a, b): s.add(ref, R[val], {"1":a,"2":b}, val)
    def cap(s, ref, val, a, b): s.add(ref, C[val], {"1":a,"2":b}, val)

core=Sheet("MCU core","core.kicad_sch")
pd  =Sheet("USB-PD","pd.kicad_sch")
hub =Sheet("USB hub + ports","hub.kicad_sch")
sbu =Sheet("SBU serial","sbu.kicad_sch")
pwr =Sheet("Power","power.kicad_sch")

# ===================== Sheet: MCU core (RP2350A) =====================
core.add("U1",MCU,{
    # power
    "1":"+3V3","11":"+3V3","20":"+3V3","30":"+3V3","38":"+3V3","45":"+3V3",  # IOVDD
    "49":"+3V3","46":"+3V3","44":"+3V3","54":"+3V3","53":"+3V3",  # VREG_VIN, VREG_AVDD, ADC_AVDD, QSPI_IOVDD, USB_OTP_VDD
    "47":"GND","61":"GND",
    "6":"CORE","23":"CORE","39":"CORE","50":"CORE","48":"VREG_LX",  # DVDD, VREG_FB, VREG_LX (switcher)
    # clock / control
    "21":"XIN","22":"XOUT","24":"SWCLK","25":"SWDIO","26":"RUN",
    # QSPI flash
    "60":"QSPI_SS","56":"QSPI_SCLK","57":"QSPI_SD0","59":"QSPI_SD1","58":"QSPI_SD2","55":"QSPI_SD3",
    # USB to hub
    "52":"MCU_DP","51":"MCU_DM",
    # GPIO (firmware map)
    "14":"SBU1_DIR","15":"SBU2_DIR","16":"SBU1_UART","17":"SBU2_UART","18":"SHIFT_EN",
    "27":"I2C_SDA","28":"I2C_SCL","31":"TGT_VBUS_EN","32":"FUSB_INT","37":"LED_STAT",
    # unused GPIO -> no-connect
    "2":None,"3":None,"4":None,"5":None,"7":None,"8":None,"9":None,"10":None,"12":None,
    "13":None,"19":None,"29":None,"33":None,"34":None,"35":None,"36":None,
    "40":None,"41":None,"42":None,"43":None,
}, "RP2350A")
core.add("L1",IND,{"1":"VREG_LX","2":"CORE"},"3.3uH")   # core switching-reg inductor
core.add("U4","W25Q32JVSSIQ_C2834491",{
    "1":"QSPI_SS","6":"QSPI_SCLK","5":"QSPI_SD0","2":"QSPI_SD1",
    "3":"QSPI_SD2","7":"QSPI_SD3","8":"+3V3","4":"GND"})
core.add("Y1",XTAL,{"1":"XIN","3":"XOUT","2":"GND","4":"GND"})
core.cap("C1","22pF","XIN","GND"); core.cap("C2","22pF","XOUT","GND")
core.add("SW1",BTN,{"1":"QSPI_SS","2":"QSPI_SS","3":"GND","4":"GND"})
core.res("R1","10k","RUN","+3V3"); core.cap("C3","100nF","RUN","GND")
core.cap("C4","10uF","CORE","GND"); core.cap("C5","100nF","CORE","GND")   # core rail
core.cap("C6","1uF","+3V3","GND"); core.cap("C7","100nF","+3V3","GND")     # VREG_VIN
core.cap("C8","100nF","+3V3","GND")                                        # VREG_AVDD
core.cap("C9","100nF","+3V3","GND"); core.cap("C10","100nF","+3V3","GND")  # IOVDD
core.cap("C11","100nF","+3V3","GND"); core.cap("C12","100nF","+3V3","GND")

# ===================== Sheet: USB-PD (FUSB302B) =====================
pd.add("U2","FUSB302BMPX",{
    "3":"+3V3","4":"+3V3","8":"GND","9":"GND","15":"GND",
    "10":"TGT_CC1","11":"TGT_CC1","1":"TGT_CC2","14":"TGT_CC2",
    "12":None,"13":None,"2":None,
    "5":"FUSB_INT","6":"I2C_SCL","7":"I2C_SDA"})
pd.res("R2","4.7k","I2C_SDA","+3V3"); pd.res("R3","4.7k","I2C_SCL","+3V3")
pd.res("R4","4.7k","FUSB_INT","+3V3")
pd.cap("C13","100nF","+3V3","GND"); pd.cap("C14","1uF","+3V3","GND")

# ===================== Sheet: USB hub + ports (CH334F) =====================
hub.add("U3","CH334F_C5187527",{
    "19":"P5V","20":"HUB_3V3","25":"GND",
    "15":"HOST_DP","14":"HOST_DM","12":"MCU_DP","11":"MCU_DM","10":"TGT_DP","9":"TGT_DM",
    "8":None,"7":None,"6":None,"5":None,
    "4":"HUB_XI","3":"HUB_XO","18":"GND","16":"HUB_RSTB","1":"HUB_3V3",
    "24":None,"23":None,"22":None,"21":None,"13":None,"17":None,"2":None})
hub.add("Y2",XTAL,{"1":"HUB_XI","3":"HUB_XO","2":"GND","4":"GND"})
hub.cap("C15","22pF","HUB_XI","GND"); hub.cap("C16","22pF","HUB_XO","GND")
hub.res("R5","10k","HUB_RSTB","HUB_3V3")
hub.cap("C17","100nF","P5V","GND"); hub.cap("C18","1uF","HUB_3V3","GND"); hub.cap("C19","100nF","HUB_3V3","GND")
hub.add("J1","TYPE-C-31-M-12",{
    "A1B12":"GND","B1A12":"GND","1":"GND","2":"GND","3":"GND","4":"GND",
    "A4B9":"P5V","B4A9":"P5V","A5":"HOST_CC1","B5":"HOST_CC2",
    "A6":"HOST_DP","A7":"HOST_DM","B6":"HOST_DP","B7":"HOST_DM","A8":None,"B8":None})
hub.res("R6","5.1k","HOST_CC1","GND"); hub.res("R7","5.1k","HOST_CC2","GND")
hub.add("J2","TYPE-C-31-M-12",{
    "A1B12":"GND","B1A12":"GND","1":"GND","2":"GND","3":"GND","4":"GND",
    "A4B9":"TGT_VBUS","B4A9":"TGT_VBUS","A5":"TGT_CC1","B5":"TGT_CC2",
    "A6":"TGT_DP","A7":"TGT_DM","B6":"TGT_DP","B7":"TGT_DM","A8":"TGT_SBU1","B8":"TGT_SBU2"})
hub.add("D10","USBLC6-2SC6",{"1":"HOST_DP","6":"HOST_DP","3":"HOST_DM","4":"HOST_DM","2":"GND","5":"P5V"})
hub.add("D11","USBLC6-2SC6",{"1":"TGT_DP","6":"TGT_DP","3":"TGT_DM","4":"TGT_DM","2":"GND","5":"TGT_VBUS"})

# ===================== Sheet: SBU 1.2V serial =====================
sbu.add("U8","74AVC1T45GW,125",{"1":"+3V3","2":"GND","3":"SBU1_UART","4":"TGT_SBU1","5":"SBU1_DIR","6":"+1V2"})
sbu.add("U9","74AVC1T45GW,125",{"1":"+3V3","2":"GND","3":"SBU2_UART","4":"TGT_SBU2","5":"SBU2_DIR","6":"+1V2"})
sbu.cap("C20","100nF","+3V3","GND"); sbu.cap("C21","100nF","+1V2","GND")

# ===================== Sheet: Power =====================
pwr.add("U5","RT9013-33GB",{"1":"P5V","2":"GND","3":"P5V","4":None,"5":"+3V3"})   # 5V->3.3V
pwr.cap("C22","10uF","P5V","GND"); pwr.cap("C23","1uF","+3V3","GND")
pwr.add("U6","TLV70212DBVR",{"1":"+3V3","2":"GND","3":"SHIFT_EN","4":None,"5":"+1V2"})  # ->1.2V
pwr.cap("C24","1uF","+1V2","GND")
pwr.add("U10","AP22653W6-7",{"1":"P5V","2":"GND","3":"TGT_VBUS_EN","4":"VBUS_FAULT","5":"ILIM_SET","6":"TGT_VBUS"})
pwr.res("R8","47k","ILIM_SET","GND"); pwr.res("R9","10k","VBUS_FAULT","+3V3")
pwr.cap("C25","100nF","P5V","GND"); pwr.cap("C26","1uF","TGT_VBUS","GND")
pwr.add("D1",LED,{"1":"LEDP_A","2":"GND"}); pwr.res("R10","1k","+3V3","LEDP_A")
pwr.add("D2",LED,{"1":"LEDS_A","2":"GND"}); pwr.res("R11","1k","LED_STAT","LEDS_A")
# power flags mark rails as driven (one per rail, on the sheet that sources it)
for i,rail in enumerate(["P5V","+3V3","+1V2","TGT_VBUS","GND"]):
    pwr.comps.append({"ref":f"#FLG{i+1}","sym":"PWR_FLAG","at":pwr._at(),"nets":{"1":rail},"value":"PWR_FLAG"})
core.comps.append({"ref":"#FLG6","sym":"PWR_FLAG","at":core._at(),"nets":{"1":"CORE"},"value":"PWR_FLAG"})
hub.comps.append({"ref":"#FLG7","sym":"PWR_FLAG","at":hub._at(),"nets":{"1":"HUB_3V3"},"value":"PWR_FLAG"})

sheets=[{"name":s.name,"file":s.file,"components":s.comps} for s in (core,pd,hub,sbu,pwr)]
gen.build_hier(sheets, "dongle-lite-hw", "dongle-lite-hw.kicad_sch")
print("sheets:", ", ".join(f'{s.name}({len(s.comps)})' for s in (core,pd,hub,sbu,pwr)))
