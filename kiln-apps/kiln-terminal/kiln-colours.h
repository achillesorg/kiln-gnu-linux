#pragma once
#include <stdbool.h>
struct config;
bool kiln_detect_dark(void);
void kiln_colours_apply(struct config *conf, bool dark);
