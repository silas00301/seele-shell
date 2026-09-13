#ifndef SEELE_CORE_H
#define SEELE_CORE_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
struct SeeleBytes { uint8_t *data; size_t length; };
// Calls are synchronous. Each returned buffer must be released exactly once.
size_t seele_core_max_message(void);
struct SeeleBytes seele_qml_call(const uint8_t *data, size_t length);
void seele_qml_free(struct SeeleBytes bytes);
void *seele_notifications_new(double now);
struct SeeleBytes seele_notifications_call(void *state, const uint8_t *data, size_t length);
void seele_notifications_free(void *state);
#ifdef __cplusplus
}
#endif
#endif
