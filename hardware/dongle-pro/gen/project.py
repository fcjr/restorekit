#!/usr/bin/env python3
"""(Re)write dongle-pro.kicad_pro from the 1s4l project template.
pcbnew.SaveBoard clobbers the sibling .kicad_pro with default project settings,
so gen/pcb-usb3.py calls write_pro() after every save. Also runnable standalone."""
import json, copy, os

HERE = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..')

def write_pro():
    p = json.load(open(os.path.join(HERE, '..', 'dongle-lite', 'dongle-lite-1s4l.kicad_pro')))
    p['meta']['filename'] = 'dongle-pro.kicad_pro'
    p['schematic']['top_level_sheets'] = [{
        'filename': 'dongle-pro.kicad_sch', 'name': 'dongle-pro',
        'uuid': '00000000-0000-0000-0000-000000000000'}]
    p['pcbnew']['last_paths']['specctra_dsn'] = 'dongle-pro.dsn'

    # USB3 SS net class: 90 ohm diff on JLC04161H-7628 outer layers (8.31/5 mil)
    default = p['net_settings']['classes'][0]
    ss = copy.deepcopy(default)
    ss.update({'name': 'USB3_SS', 'priority': 0,
               'track_width': 0.211, 'clearance': 0.127,
               'diff_pair_width': 0.211, 'diff_pair_gap': 0.127,
               'diff_pair_via_gap': 0.127})
    p['net_settings']['classes'].append(ss)
    # Power distribution: fatter tracks (no planes on this 4-layer; pours are GND)
    pwr = copy.deepcopy(default)
    pwr.update({'name': 'PWR', 'priority': 1, 'track_width': 0.3})
    p['net_settings']['classes'].append(pwr)
    p['net_settings']['netclass_patterns'] = (
        [{'netclass': 'USB3_SS', 'pattern': pat} for pat in
         ['HOST_TX*', 'HOST_RX*', 'TGT_TX*', 'TGT_RX*',
          'HUB_UTX*', 'HUB_URX*', 'HUB_DTX*', 'HUB_DRX*', 'M1_TX*', 'M2_A0*']] +
        [{'netclass': 'PWR', 'pattern': pat} for pat in
         ['+5V', '+3V3', '+1V1', '+1V2', 'HUB_3V3', 'HUB_1V2', 'HUB_SW',
          'HUB_AV12', 'VREG_LX', 'TGT_VBUS']])

    # ERC policy per README: LCSC symbols mistype passive pins as Input/Unspecified
    # 0.4/0.5mm-pitch QFN/USON parts + TVS thru-links make KiCad's mask-bridge
    # check fire on adjacent pads; JLCPCB's mask rules handle these. Downgrade.
    p['board']['design_settings']['rule_severities']['solder_mask_bridge'] = 'warning'
    p['erc'] = {'erc_exclusions': [], 'meta': {'version': 0},
                'rule_severities': {
                    'pin_not_driven': 'ignore', 'pin_to_pin': 'ignore',
                    'lib_symbol_issues': 'ignore', 'lib_symbol_mismatch': 'ignore'}}

    json.dump(p, open(os.path.join(HERE, 'dongle-pro.kicad_pro'), 'w'), indent=2)

if __name__ == '__main__':
    write_pro()
    print('dongle-pro.kicad_pro written')
