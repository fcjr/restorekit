#ifndef RK_SYS_UN_H
#define RK_SYS_UN_H
#include "_rk_win_sockcompat.h"
struct sockaddr_un { unsigned short sun_family; char sun_path[108]; };
#endif
