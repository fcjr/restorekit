/* BSD-style TCP header for usbmuxd's TCP-over-USB muxer (Windows has none). */
#ifndef RK_NETINET_TCP_H
#define RK_NETINET_TCP_H

#include "_rk_win_sockcompat.h"
#include <stdint.h>

struct tcphdr {
	uint16_t th_sport;
	uint16_t th_dport;
	uint32_t th_seq;
	uint32_t th_ack;
	/* x86_64 is little-endian: reserved bits before the data offset. */
	uint8_t th_x2 : 4;
	uint8_t th_off : 4;
	uint8_t th_flags;
	uint16_t th_win;
	uint16_t th_sum;
	uint16_t th_urp;
};

#ifndef TH_FIN
#define TH_FIN 0x01
#define TH_SYN 0x02
#define TH_RST 0x04
#define TH_PUSH 0x08
#define TH_ACK 0x10
#define TH_URG 0x20
#endif

#endif /* RK_NETINET_TCP_H */
