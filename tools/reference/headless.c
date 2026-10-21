#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "ffeapi.h"

extern int asmmain(int argc, char **argv);
extern unsigned char ref_state[];
extern unsigned long ref_rng(void);
extern void ref_timer_advance(void);
extern unsigned long ref_system_counter, ref_accel_counter, ref_time_accel;
extern unsigned long ref_timer_last, ref_timer_next, ref_timer_min;
extern unsigned char *ref_framebuffer;
extern int ref_draw_char(int glyph, int colour, int x, int y, int min_y, int max_y);
extern int ref_char_width(int code);
extern void *ref_blit_index(void *buffer, int index, int x, int y);
extern unsigned long *ref_model(int index);
extern long *ref_vertex(void *instance, int index);
extern long *ref_vertex_buffer;
extern void *ref_command_math(void *instance, void *operands, unsigned long code);
extern unsigned char *ref_command_clear(void *instance, void *operands, unsigned long code);
extern unsigned char *ref_command_set(void *instance, void *operands, unsigned long code);
extern unsigned long ref_command_alive;
extern void *ref_material(void *output, long value);
extern void *ref_triangle(void *instance, void *operands, unsigned long code);
extern void *ref_mirror_triangle(void *instance, void *operands, unsigned long code);
extern void *ref_quad(void *instance, void *operands, unsigned long code);
extern void *ref_mirror_quad(void *instance, void *operands, unsigned long code);
extern unsigned char *ref_normal(void *instance, int index);
extern void ref_normal_init(void *instance, void *model);
extern unsigned char *ref_normal_buffer;
extern long ref_sin32(long angle);
extern void ref_rotate(void *instance, long axis, long cosine, long sine);
extern long ref_angle(void *instance, long encoded);
extern unsigned char *ref_rotate_command(void *instance, void *operands, unsigned long code);
extern unsigned char *ref_transform_command(void *instance, void *operands, unsigned long code);
extern long ref_visibility(void *instance,long operand);
extern unsigned char *ref_visibility_branch(void *instance,void *operands,unsigned long code);
extern unsigned char *ref_visibility_inverse(void *instance,void *operands,unsigned long code);
extern long ref_detail_primary,ref_detail_secondary;
extern void ref_execute(void *instance,void *commands);
extern unsigned long ref_depth_key(void *instance,const long *point);
extern unsigned char *ref_depth_command(void *instance,void *operands,unsigned long code);
extern unsigned long *ref_order_base,*ref_order_cursor;
extern unsigned long ref_order_active,ref_order_state;
extern void ref_order_init(void *buffer);
extern void ref_order_group(long active,unsigned long key);
extern unsigned long *ref_order_allocate(long words,unsigned long key,long force);
extern void ref_order_flush(void);
extern long *ref_view_vertex(void *instance,int index);
extern long *ref_project_vertex(void *instance,int index);
extern long ref_project_point(void *output,const long *point);

/* Only the separate command probe redirects the four primitive-emission calls.
   These callbacks capture original handler arguments, not native decoder output. */
static int probe_face_count;
/* Queue mode: faces run through the real F1708/F1712, and every face allocation
   (F1778 within F1708..F1714) and depth-group change (F1780 within F1696) is logged
   before reaching the original allocator. */
extern int ref_face3(void *instance, long material, unsigned long a, unsigned long b, unsigned long c, unsigned long normal);
extern int ref_face4(void *instance, long material, unsigned long a, unsigned long b, unsigned long c, unsigned long d, unsigned long normal);
extern void ref_order_group(long active,unsigned long key);
extern unsigned long *ref_order_allocate(long words,unsigned long key,long force);
static int queue_mode;
static unsigned long queue_count, queue_log[1024][3];
static void queue_event(unsigned long kind, unsigned long active, unsigned long key)
{
    if (queue_count >= 1024) { fprintf(stderr,"too many queue events\n"); exit(2); }
    queue_log[queue_count][0]=kind; queue_log[queue_count][1]=active; queue_log[queue_count][2]=key;
    queue_count++;
}
void trace_group(long active, unsigned long key)
{
    if (queue_mode) queue_event(2,active!=0,active ? key : 0);
    ref_order_group(active,key);
}
unsigned long *trace_alloc(long words, unsigned long key, long force)
{
    if (queue_mode) queue_event(1,0,key);
    return ref_order_allocate(words,key,force);
}
static unsigned long probe_faces[2][6];
static int execution_probe;
static unsigned long execution_faces;
static uint64_t execution_hash;
static void execution_emission(void *instance,const unsigned long *face);
int trace_triangle(void *instance, long material, unsigned long a, unsigned long b, unsigned long c, unsigned long normal)
{
    unsigned long *face;
    if(queue_mode) return ref_face3(instance,material,a,b,c,normal);
    if(execution_probe) {
        unsigned long emitted[6]={(unsigned short)material,normal,a,b,c,255};
        execution_emission(instance,emitted);return 0;
    }
    if (probe_face_count >= 2) { fprintf(stderr,"too many face callbacks\n"); exit(2); }
    face = probe_faces[probe_face_count++];
    face[0]=(unsigned short)material; face[1]=normal;
    face[2]=a; face[3]=b; face[4]=c; face[5]=255;
    return 0;
}
int trace_quad(void *instance, long material, unsigned long a, unsigned long b, unsigned long c, unsigned long d, unsigned long normal)
{
    unsigned long *face;
    if(queue_mode) return ref_face4(instance,material,a,b,c,d,normal);
    if(execution_probe) {
        unsigned long emitted[6]={(unsigned short)material,normal,a,b,c,d};
        execution_emission(instance,emitted);return 0;
    }
    if (probe_face_count >= 2) { fprintf(stderr,"too many face callbacks\n"); exit(2); }
    face = probe_faces[probe_face_count++];
    face[0]=(unsigned short)material; face[1]=normal;
    face[2]=a; face[3]=b; face[4]=c; face[5]=d;
    return 0;
}

static unsigned long point_line_count, point_line_kind[16], point_line_a[16], point_line_b[16], point_line_c[16], point_line_d[16];
void trace_point_args(void *instance, long material, long angle, long vertex, long flag)
{
    (void)instance;
    if (point_line_count >= 16) { fprintf(stderr,"too many point callbacks\n"); exit(2); }
    point_line_kind[point_line_count]=1;
    point_line_a[point_line_count]=(unsigned long)material;
    point_line_b[point_line_count]=(unsigned long)angle;
    point_line_c[point_line_count]=(unsigned long)vertex;
    point_line_d[point_line_count]=(unsigned long)flag;
    point_line_count++;
}

void trace_line_args(void *instance, long material, long a, long b)
{
    (void)instance;
    if (point_line_count >= 16) { fprintf(stderr,"too many line callbacks\n"); exit(2); }
    point_line_kind[point_line_count]=2;
    point_line_a[point_line_count]=(unsigned long)material;
    point_line_b[point_line_count]=(unsigned long)a;
    point_line_c[point_line_count]=(unsigned long)b;
    point_line_count++;
}

extern long *ref_view_vertex(void *instance,int index);
extern long *ref_project_vertex(void *instance,int index);

/* Scoped text (F1747) and submodel (F1749/F1750) probes. Nested routines are
   replaced by recorders returning synthetic descriptors/vertices; the original
   handlers still decode their operands and compute the returned cursor. */
static unsigned long nested_count, nested_kind[256], nested_value[256][3];
static long nested_vertex[9], nested_descriptor[16];
/* Station mode: vertex hooks resolve through the real F1470/F1471, text skips its
   glyphs (F1745 reports no room) and child descriptors always cull (negative radius). */
static int nested_station;
static void nested_event(unsigned long kind, unsigned long a, unsigned long b, unsigned long c)
{
    if (nested_count >= 256) { fprintf(stderr,"too many nested probe callbacks\n"); exit(2); }
    nested_kind[nested_count]=kind;
    nested_value[nested_count][0]=a; nested_value[nested_count][1]=b; nested_value[nested_count][2]=c;
    nested_count++;
}
long *trace_text_vertex(void *instance, long index) { nested_event(1,(unsigned char)index,0,0); return nested_station ? ref_view_vertex(instance,(signed char)index) : nested_vertex; }
long *trace_text_font(long selector) { nested_event(2,(unsigned long)selector,0,0); return nested_descriptor; }
void trace_text_frame(void *child, void *instance, long word) { (void)child; (void)instance; nested_event(3,(unsigned long)word,0,0); }
long trace_text_push(long vertices, long normals, long extra) { nested_event(4,(unsigned long)vertices,(unsigned long)normals,(unsigned long)extra); return nested_station; }
void trace_text_string(char *output, long string, void *scratch) { (void)scratch; output[0]=0; nested_event(5,(unsigned long)string,0,0); }
void trace_text_pop(void) { nested_event(6,0,0,0); }
long *trace_submodel_vertex(void *instance, long index) { nested_event(7,(unsigned char)index,0,0); return nested_station ? ref_project_vertex(instance,(signed char)index) : nested_vertex; }
long *trace_submodel_model(long index) { nested_event(8,(unsigned long)index,0,0); return nested_descriptor; }
void trace_submodel_frame(void *child, void *instance, long word) { (void)child; (void)instance; nested_event(9,(unsigned long)word,0,0); }
void trace_submodel_run(void *child, void *descriptor) { (void)child; (void)descriptor; nested_event(10,0,0,0); }

static unsigned char pixels[320 * 200];
static unsigned char palette[256 * 3];
static unsigned long frames, limit = 32, milliseconds, clock_reads, blits;
static int sent_accept;
static int state_mode, vector_mode, sent_start;

static unsigned long state_u32(size_t offset)
{
    unsigned long value;
    memcpy(&value, ref_state + offset, 4);
    return value;
}

static void state_set_u32(size_t offset, unsigned long value)
{
    memcpy(ref_state + offset, &value, 4);
}

static uint64_t fingerprint(const unsigned char *data, size_t size)
{
    uint64_t hash = UINT64_C(14695981039346656037);
    size_t i;
    for (i = 0; i < size; ++i) {
        hash ^= data[i];
        hash *= UINT64_C(1099511628211);
    }

    return hash;
}

static void execution_hash_bytes(const unsigned char *data,size_t size)
{
    size_t i;
    for(i=0;i<size;++i) {execution_hash^=data[i];execution_hash*=UINT64_C(1099511628211);}
}

static void execution_emission(void *instance,const unsigned long *face)
{
    if(++execution_faces>256) {fprintf(stderr,"unbounded execution probe emissions\n");exit(2);}
    execution_hash_bytes((const unsigned char *)face,24);
    execution_hash_bytes((const unsigned char *)instance+0x54,32);
    execution_hash_bytes((const unsigned char *)instance+0x9c,36);
}

static void fail(const char *message)
{
    fprintf(stderr, "reference error: %s\n", message);
    exit(2);
}

static void rectangle(long x, long y, long w, long h, long jump)
{
    if (x < 0 || y < 0 || w < 0 || h < 0 || x > 320 || y > 200 ||
        w > 320 - x || h > 200 - y || jump < 0 || jump > 1048576)
        fail("invalid blit rectangle");
}

void SystemInit(void)
{
    if (state_mode)
        fprintf(stderr, "JJFFE headless state adapter: no audio, no video device, no saves; synthetic clock, scripted Y and choice 1. Not a gameplay oracle.\n");
    else
        fprintf(stderr, "JJFFE headless smoke adapter: no audio, no video device, no saves; synthetic clock, scripted Y acceptance only. Not a gameplay oracle.\n");
}
void SystemCleanup(void) {}

void VideoBlit(UCHAR *data, long x, long y, long w, long h, long jump)
{
    long row;
    rectangle(x, y, w, h, jump);
    for (row = 0; row < h; ++row)
        memcpy(pixels + (y + row) * 320 + x, data + row * (w + jump), w);
    ++blits;
}
void VideoMaskedBlit(UCHAR *data, long x, long y, long w, long h, long jump)
{
    long row, col;
    rectangle(x, y, w, h, jump);
    for (row = 0; row < h; ++row)
        for (col = 0; col < w; ++col)
            if (data[row * (w + jump) + col])
                pixels[(y + row) * 320 + x + col] = data[row * (w + jump) + col];
}
void VideoReverseBlit(UCHAR *data, long x, long y, long w, long h, long jump)
{
    long row;
    rectangle(x, y, w, h, jump);
    for (row = 0; row < h; ++row)
        memcpy(data + row * (w + jump), pixels + (y + row) * 320 + x, w);
}
void VideoGetPalValue(long index, UCHAR *value)
{
    if (index < 0 || index >= 256) fail("invalid palette index");
    memcpy(value, palette + index * 3, 3);
}
void VideoSetPalValue(long index, UCHAR *value)
{
    if (index < 0 || index >= 256) fail("invalid palette index");
    memcpy(palette + index * 3, value, 3);
}
long VideoPointerExclusive(void) { return 0; }
void VideoPointerEnable(void) {}
void VideoPointerDisable(void) {}
long InputMouseReadButtons(void) { return 0; }
void InputMouseReadMickeys(long *x, long *y) { *x = 0; *y = 0; }
void InputMouseReadPos(long *x, long *y) { *x = 160; *y = 100; }
void InputJoyReadPos(long *x, long *y) { *x = 0; *y = 0; }
long InputJoyReadButtons(void) { return 0; }
void InputKeybReadStates(UCHAR *keys) { memset(keys, 0x80, 256); }
long InputKeybGetLastKey(void)
{
    if (!sent_accept && frames >= 2) {
        sent_accept = 1;
        return 0x15;
    }
    if (state_mode && !sent_start && frames >= 160) {
        sent_start = 1;
        return 0x02; /* Intro choice 1, after the 100-tick splash lockout. */
    }
    return 0xff;
}
void InputKeybSetLastKey(long key) { (void)key; }
ULONG TimerGetTimeStamp(void)
{
    if (++clock_reads > 1000000) fail("clock-read bound exceeded");
    if (vector_mode) return milliseconds;
    return milliseconds++;
}
void TimerSleep(void) { ++milliseconds; }
void TimerFrameUpdate(void)
{
    if (vector_mode) return;
    if (state_mode) {
        printf("{\"frame\":%lu,\"clock_ms\":%lu,\"game_tics\":%lu,\"game_days\":%lu,\"rng_a\":%lu,\"rng_b\":%lu,\"player_state\":%u,\"player_index\":%lu,\"cash\":%lu,\"start_type\":%lu,\"time_accel\":%lu,\"system_counter\":%lu,\"accel_counter\":%lu,\"pixels_fnv1a64\":\"%016llx\",\"palette_fnv1a64\":\"%016llx\"}\n",
            frames, milliseconds, state_u32(0), state_u32(4), state_u32(0x24), state_u32(0x28),
            (unsigned)ref_state[0xe8], state_u32(0xc0), state_u32(0x114), state_u32(0xfa),
            ref_time_accel, ref_system_counter, ref_accel_counter,
            (unsigned long long)fingerprint(pixels, sizeof(pixels)),
            (unsigned long long)fingerprint(palette, sizeof(palette)));
    } else {
        printf("{\"frame\":%lu,\"clock_ms\":%lu,\"blits\":%lu,\"pixels_fnv1a64\":\"%016llx\",\"palette_fnv1a64\":\"%016llx\"}\n",
            frames, milliseconds, blits, (unsigned long long)fingerprint(pixels, sizeof(pixels)),
            (unsigned long long)fingerprint(palette, sizeof(palette)));
    }
    if (fflush(stdout)) fail("cannot flush trace");
    if (++frames >= limit) exit(0);
}
void SoundCheckInit(long *all, long *digi, long *midi) { *all = 0; *digi = 0; *midi = 0; }
void SoundPlaySong(long song) { (void)song; }
void SoundStopSong(void) {}
long SoundSongDone(void) { return 1; }
void SoundStopAllSamples(void) {}
void SoundPlaySample(long sample, long volume, long pitch)
{ (void)sample; (void)volume; (void)pitch; }
long SoundStreamGetUsedBlocks(void) { return 0; }
void SoundStreamReset(void) {}
void SoundStreamAddBlock(void *data, long size) { (void)data; (void)size; }
void SoundStreamStart(void) {}
void SoundStreamWait(void) {}
void SoundStreamStop(void) {}
char *DirMakeAVIName(char *buffer, char *stub)
{
    (void)buffer; (void)stub;
    fail("video decoding is unsupported by this smoke adapter");
    return NULL;
}
char *DirMakeCmmdrName(char *buffer, char *name)
{
    (void)buffer; (void)name;
    fail("save/load is unsupported by this smoke adapter");
    return NULL;
}
char *DirGetCmmdrPath(void) { return "COMMANDR\\"; }
void DirResetCmmdrPath(void) {}
void DirNavigateTree(FileInfo *file) { (void)file; fail("save browsing is unsupported"); }
int DirFindFirst(FileInfo *file) { (void)file; return 0; }
int DirFindNext(FileInfo *file) { (void)file; return 0; }

static int vectors(void)
{
    static const unsigned long seeds[][2] = {
        {0, 0}, {0x12345678, 0x456789ab}, {0x12345678, 0x89abcdef},
        {0xffffffff, 0xffffffff}, {0x80000000, 1}, {1, 0x80000000}
    };
    static const unsigned long elapsed[] = {0, 1, 5, 19, 20, 21, 39, 40, 100, 1000};
    static const unsigned long acceleration[] = {0, 1, 994, 1000, 10000, 1000000};
    size_t scenario, index, accel;
    unsigned long a, b, value, initial_system, initial_accel;
    vector_mode = 1;
    fprintf(stderr, "JJFFE numeric reference rng-timer-v1: actual legacy routines, controlled counter inputs; not flight physics.\n");
    for (scenario = 0; scenario < sizeof(seeds) / sizeof(seeds[0]); ++scenario) {
        state_set_u32(0x24, seeds[scenario][0]);
        state_set_u32(0x28, seeds[scenario][1]);
        for (index = 0; index < 256; ++index) {
            a = state_u32(0x24);
            b = state_u32(0x28);
            value = ref_rng();
            printf("{\"kind\":\"rng\",\"scenario\":%u,\"step\":%u,\"before_a\":%lu,\"before_b\":%lu,\"after_a\":%lu,\"after_b\":%lu,\"value\":%lu}\n",
                (unsigned)scenario, (unsigned)index, a, b, state_u32(0x24), state_u32(0x28), value);
        }
    }
    for (accel = 0; accel < sizeof(acceleration) / sizeof(acceleration[0]); ++accel) {
        for (index = 0; index < sizeof(elapsed) / sizeof(elapsed[0]); ++index) {
            /* Avoid TimerInit/config and exercise TimerAdvance itself, without
               pacing sleeps or mixing its counter math with the game loop. */
            ref_timer_min = 0;
            ref_timer_last = 100;
            ref_timer_next = 100;
            milliseconds = 100 + elapsed[index];
            ref_time_accel = acceleration[accel];
            initial_system = index % 2 ? 0xfffffff0 : 0;
            initial_accel = index % 2 ? 0xffffffe0 : 0;
            ref_system_counter = initial_system;
            ref_accel_counter = initial_accel;
            ref_timer_advance();
            printf("{\"kind\":\"timer\",\"elapsed_ms\":%lu,\"acceleration\":%lu,\"before_system\":%lu,\"before_accel\":%lu,\"after_system\":%lu,\"after_accel\":%lu,\"last_ms\":%lu,\"next_50hz_ms\":%lu}\n",
                elapsed[index], acceleration[accel], initial_system, initial_accel,
                ref_system_counter, ref_accel_counter, ref_timer_last, ref_timer_next);
        }
    }
    if (fflush(stdout)) fail("cannot flush numeric vectors");
    return 0;
}

static int font_vectors(void)
{
    static const int cases[][3] = {{0,0,200},{157,0,200},{195,0,200},{-4,0,200},{3,5,8},{200,0,200}};
    int code, advance, width;
    size_t index;
    ref_framebuffer = pixels;
    fprintf(stderr, "JJFFE font-v1: executed DrawChar/CharAdvanceWidth, all 116 glyphs, six clipping cases.\n");
    for (code=32; code<148; ++code) {
        width = ref_char_width(code);
        for (index=0; index<sizeof(cases)/sizeof(cases[0]); ++index) {
            memset(pixels, 3, sizeof(pixels));
            advance = ref_draw_char(code-32, 77, 7, cases[index][0], cases[index][1], cases[index][2]);
            printf("{\"glyph\":%d,\"x\":7,\"y\":%d,\"min_y\":%d,\"max_y\":%d,\"advance\":%d,\"width\":%d,\"pixels_fnv1a64\":\"%016llx\"}\n",
                code,cases[index][0],cases[index][1],cases[index][2],advance,width,
                (unsigned long long)fingerprint(pixels,sizeof(pixels)));
        }
    }
    if (fflush(stdout)) fail("cannot flush font vectors");
    return 0;
}

static int blit_vectors(void)
{
    static const int slots[] = {43,0,44,66};
    static const int cases[][2] = {{0,158},{-5,-3},{319,199},{0,0},{320,200},{-400,-300},{-1,170}};
    unsigned long buffer[6];
    size_t slot,index;
    memset(buffer,0,sizeof(buffer));
    buffer[0]=(unsigned long)pixels;
    buffer[3]=320;
    buffer[4]=200;
    fprintf(stderr,"JJFFE blit-v1: executed BlitIndex, opaque console and masked sprites, seven clipping cases.\n");
    for (slot=0;slot<sizeof(slots)/sizeof(slots[0]);++slot) {
        for (index=0;index<sizeof(cases)/sizeof(cases[0]);++index) {
            memset(pixels,3,sizeof(pixels));
            ref_blit_index(buffer,slots[slot],cases[index][0],cases[index][1]);
            printf("{\"slot\":%d,\"x\":%d,\"y\":%d,\"pixels_fnv1a64\":\"%016llx\"}\n",
                slots[slot],cases[index][0],cases[index][1],
                (unsigned long long)fingerprint(pixels,sizeof(pixels)));
        }
    }
    if (fflush(stdout)) fail("cannot flush bitmap vectors");
    return 0;
}

static int model_vectors(void)
{
    static const int models[] = {74,75};
    static const long animation[] = {-65536,0,1,16384,32768,65535,65536,2147483647};
    static const long matrices[][9] = {
        {2147483647,0,0,0,2147483647,0,0,0,2147483647},
        {0,2147483647,0,-2147483647,0,0,0,0,2147483647},
        {1073741824,-536870912,268435456,536870912,1073741824,-268435456,-268435456,268435456,1073741824},
        {(-2147483647-1),0,0,0,(-2147483647-1),0,0,0,(-2147483647-1)}
    };
    unsigned long instance[0x160/4], globals[0x120/4];
    long vertices[256*9], coordinates[128*3];
    size_t m,mat,s,a,i;
    fprintf(stderr,"JJFFE model-vertices-v1: F1472/F1477/F1521, two station vertex programs; no drawing commands/projection.\n");
    for (m=0;m<2;++m) {
        unsigned long *model = ref_model(models[m]);
        unsigned long scales[] = {0,model[5],31};
        if (model[2]>128 || model[2]==0) fail("invalid model vector count");
        for (mat=0;mat<4;++mat) for (s=0;s<3;++s) for (a=0;a<8;++a) {
            memset(instance,0,sizeof(instance));
            memset(globals,0,sizeof(globals));
            memset(vertices,0,sizeof(vertices));
            ref_vertex_buffer = vertices+128*9;
            memcpy((char *)instance+0x14,matrices[mat],36);
            for (i=0;i<64;++i) ((unsigned short *)((char *)globals+0x9c))[i]=(unsigned short)((unsigned long)animation[a]+i*257);
            for (i=0;i<32;++i) instance[0x54/4+i]=(unsigned long)animation[a]^(i*0x10203);
            instance[0xf0/4]=1;
            instance[0x148/4]=scales[s];
            instance[0x14c/4]=(unsigned long)globals;
            instance[0x150/4]=model[1];
            for (i=0;i<model[2];++i) {
                long *vertex=ref_vertex(instance,(int)i);
                memcpy(coordinates+i*3,vertex+6,12);
            }
            printf("{\"model\":%d,\"matrix\":%u,\"scale\":%lu,\"animation\":%ld,\"vertices\":%lu,\"coordinates_fnv1a64\":\"%016llx\"}\n",
                models[m],(unsigned)mat,scales[s],animation[a],model[2],
                (unsigned long long)fingerprint((unsigned char *)coordinates,model[2]*12));
        }
    }
    if (fflush(stdout)) fail("cannot flush model vectors");
    return 0;
}

static int normal_vectors(void)
{
    static const int models[]={74,75};
    static const long observers[][3]={{0,0,0},{0,0,100},{0,0,-100},{100,50,-25},{-100,-50,25},
        {(-2147483647-1),2147483647,-1},{65536,-65536,16384}};
    static const long lights[][3]={{0,0,0},{0,0,-65536},{-65536,32768,16384},{(-2147483647-1),2147483647,-1}};
    unsigned long instance[0x160/4], buffer[256*2];
    unsigned char semantic[256*4];
    unsigned int m,o,l,s,t,i,j;
    fprintf(stderr,"JJFFE normals-v1: executed F1480/F1481 visibility and lighting, both stations; no full command execution or pixels.\n");
    for (m=0;m<2;++m) {
        unsigned long *model=ref_model(models[m]);
        unsigned long scales[]={0,model[5],31};
        if (model[4]<2 || model[4]>256) fail("invalid normal count");
        for (s=0;s<3;++s) for (t=0;t<3;++t) for (o=0;o<7;++o) for (l=0;l<4;++l) {
            memset(instance,0,sizeof(instance));memset(buffer,0xff,sizeof(buffer));memset(semantic,0,sizeof(semantic));
            ref_normal_buffer=(unsigned char *)buffer;
            memcpy((char *)instance+0x38,lights[l],12);
            instance[0x44/4]=scales[t];
            memcpy((char *)instance+0x48,observers[o],12);
            for (i=0;i<8;++i) for (j=0;j<3;++j) ((unsigned char *)instance)[0x84+i*3+j]=(unsigned char)(i*17+j*3+1);
            instance[0x148/4]=scales[s];instance[0x150/4]=model[1];instance[0x154/4]=model[3];
            ref_normal_init(instance,model);
            for (i=0;i<model[4];++i) {
                unsigned char *normal=i<2 ? ref_normal_buffer+i*8 : ref_normal(instance,i);
                memcpy(semantic+i*4,normal,3);semantic[i*4+3]=normal[3]&3;
            }
            printf("{\"model\":%d,\"scale\":%lu,\"observer_scale\":%lu,\"observer\":%u,\"light\":%u,\"normals\":%lu,\"hash\":\"%016llx\"}\n",
                models[m],scales[s],scales[t],o,l,model[4],(unsigned long long)fingerprint(semantic,model[4]*4));
        }
    }
    if (fflush(stdout)) fail("cannot flush normal vectors");
    return 0;
}

static int depth_vectors(void)
{
    static const long values[]={(-2147483647-1),-65536,-1,0,1,7,8,65536,0x3fffffff,0x40000000,2147483647};
    static const long scales[]={(-2147483647-1),-33,-29,-1,0,3,4,31,32,34,35,36,2147483647};
    unsigned long instance[0x160/4],s,x,y,z;
    long point[3];
    memset(instance,0,sizeof(instance));
    fprintf(stderr,"JJFFE depth-v1: original F1777 render-order key, not original sorting or rasterization.\n");
    for(s=0;s<13;++s) for(x=0;x<11;++x) for(y=0;y<11;++y) for(z=0;z<11;++z) {
        instance[0x130/4]=(unsigned long)scales[s];
        point[0]=values[x];point[1]=values[y];point[2]=values[z];
        printf("{\"scale\":%ld,\"point\":[%ld,%ld,%ld],\"key\":%lu}\n",
               scales[s],point[0],point[1],point[2],ref_depth_key(instance,point));
    }
    return 0;
}

static int projection_vectors(void)
{
    static const long values[]={(-2147483647-1),-65537,-1,0,1,63,64,65535,65536,65537,0x1f0000,0x40000000,2147483647};
    unsigned long x,y,z;
    long point[3],result;
    short screen[2];
    fprintf(stderr,"JJFFE projection-v1: original F1479 integer projection, not clipping or rasterization.\n");
    for(x=0;x<13;++x) for(y=0;y<13;++y) for(z=0;z<13;++z) {
        point[0]=values[x];point[1]=values[y];point[2]=values[z];
        screen[0]=1234;screen[1]=-1234;
        result=ref_project_point(screen,point);
        printf("{\"point\":[%ld,%ld,%ld],\"rejected\":%ld,\"screen\":[%d,%d]}\n",
            point[0],point[1],point[2],result,(int)screen[0],(int)screen[1]);
    }
    return 0;
}

static int live_vertex_vectors(void)
{
    static const unsigned char records[]={
        1,0,4,2,8,0, 0,0,8,4,16,0, 9,0,3,5,9,0, 10,0,7,11,13,0,
        5,0,0,0,0,0, 11,0,0,0,2,0, 15,0,2,0,4,0, 17,0,0,0,2,0,
        19,0,0xc8,0,2,0, 20,0,0x81,2,4,0, 6,0,0,0,2,0, 2,0,255,127,128,0
    };
    static const long matrices[][9]={
        {2147483647,0,0,0,2147483647,0,0,0,2147483647},
        {0,2147483647,0,-2147483647,0,0,0,0,2147483647},
        {1073741824,-536870912,268435456,536870912,1073741824,-268435456,-268435456,268435456,1073741824},
        {(-2147483647-1),0,0,0,(-2147483647-1),0,0,0,(-2147483647-1)}
    };
    static const long positions[][3]={{0,0,0},{0,0,64},{100,-200,65536},{-100,200,65537},
        {(-2147483647-1),2147483647,-1},{2147483647,(-2147483647-1),0x40000000}};
    static const unsigned long scales[]={0,10,31};
    unsigned long instance[0x160/4],globals[0x120/4],slots[24*9],seed,state,i,index,projected,generation,j;
    long object[24*3],view[24*3];
    short screen[24*2];
    unsigned long stamps[24];
    fprintf(stderr,"JJFFE live-vertices-v1: original command cache check and F1470/F1471/F1478/F1479; synthetic vertex records, explicit clears, no external vertices or screen-derived forms.\n");
    for(seed=1;seed<=128;++seed) {
        memset(instance,0,sizeof(instance));memset(globals,0,sizeof(globals));
        ref_vertex_buffer=(long *)slots;
        instance[0x148/4]=scales[seed%3];instance[0x150/4]=(unsigned long)records;
        instance[0x14c/4]=(unsigned long)globals;
        state=seed;
        for(i=0;i<64;++i) {
            if(i%16==0) memset(slots,0,sizeof(slots));
            state=state*1664525+1013904223;
            index=(state>>16)%24;projected=(state>>24)&1;generation=2+(i/8)%3;
            instance[0xf0/4]=generation;
            memcpy((char *)instance+0x14,matrices[(seed+i/8)%4],36);
            memcpy((char *)instance+4,positions[(seed+i/4)%6],12);
            instance[0x54/4+8]=(i/8)*16384;
            ((unsigned short *)((char *)globals+0x9c))[1]=(unsigned short)((i/8)*8191);
            if(slots[index*9+5]!=generation) {
                if(projected) ref_project_vertex(instance,(int)index);
                else ref_view_vertex(instance,(int)index);
            }
            for(j=0;j<24;++j) {
                memcpy(object+j*3,slots+j*9+6,12);
                memcpy(view+j*3,slots+j*9+1,12);
                memcpy(screen+j*2,slots+j*9,4);
                stamps[j]=slots[j*9+5];
            }
            printf("{\"seed\":%lu,\"step\":%lu,\"index\":%lu,\"projected\":%lu,\"generation\":%lu,\"object\":\"%016llx\",\"view\":\"%016llx\",\"screen\":\"%016llx\",\"stamps\":\"%016llx\"}\n",
                seed,i,index,projected,generation,fingerprint((unsigned char *)object,sizeof(object)),
                fingerprint((unsigned char *)view,sizeof(view)),fingerprint((unsigned char *)screen,sizeof(screen)),
                fingerprint((unsigned char *)stamps,sizeof(stamps)));
        }
    }
    return 0;
}

static unsigned long order_buffer[1024],order_output[64],order_count;
static unsigned long composed_keys[64];

static void order_init(void)
{
    memset(order_buffer,0,sizeof(order_buffer));
    ref_order_base=order_buffer;
    ref_order_active=0;
    ref_order_state=0;
    ref_order_init(order_buffer);
    order_count=0;
}

static unsigned long *order_marker(unsigned long *record)
{
    if(record<order_buffer || record+2>ref_order_cursor || order_count>=64)
        fail("invalid original order callback");
    order_output[order_count++]=record[1];
    ref_order_state=1;
    return record+2;
}

static int depth_command_vectors(void)
{
    static const long scales[]={(-2147483647-1),-33,-29,-1,0,3,4,31,32,34,35,36,2147483647};
    static const long shifts[]={(-2147483647-1),-33,-1,0,1,15,31,32,33,2147483647};
    static const unsigned short codes[]={0x0006,0x1fe6,0x2046,0x2066,0x4046,0x6046,0x6066,0x8006,0xa006,0xc006,0xe046,0xe066,0xffa6,0xffc6,0xffe6};
    static const long depths[]={1,0x40000000,(-2147483647-1)};
    unsigned long instance[0x160/4],vertices[8*9],s,t,p,c,initial,i,number=0,words;
    unsigned char operands[]={4,255,6,0};
    unsigned char *next;
    fprintf(stderr,"JJFFE depth-commands-v1: original F1696 with primed view-space vertex cache and original F1780 group allocator; not lazy vertex evaluation or pixels.\n");
    for(s=0;s<13;++s) for(t=0;t<10;++t) for(p=0;p<3;++p)
    for(c=0;c<15;++c) for(initial=0;initial<2;++initial) {
        memset(instance,0,sizeof(instance));memset(vertices,0,sizeof(vertices));
        instance[0xf0/4]=1;instance[0x130/4]=(unsigned long)scales[s];
        instance[0x148/4]=(unsigned long)shifts[t];
        instance[1]=0x80000000;instance[2]=65536;instance[3]=(unsigned long)depths[p];
        for(i=0;i<8;++i) {
            vertices[i*9+1]=(i&1)?0x80000000:i*8;
            vertices[i*9+2]=(unsigned long)(-(long)i*65536);
            vertices[i*9+3]=(unsigned long)depths[(p+i)%3];
            vertices[i*9+5]=1;
        }
        ref_vertex_buffer=(long *)vertices;
        order_init();
        if(initial) ref_order_group(1,99);
        next=ref_depth_command(instance,operands,codes[c]);
        words=(unsigned long)(ref_order_cursor-order_buffer);
        printf("{\"case\":%lu,\"scale\":%ld,\"shift\":%ld,\"point_set\":%lu,\"code\":%u,\"initial\":%lu,\"consumed\":%ld,\"active\":%lu,\"words\":%lu,\"key\":",
            number++,scales[s],shifts[t],p,(unsigned)codes[c],initial,(long)(next-operands),ref_order_active,words);
        if(ref_order_active) printf("%lu",ref_order_cursor[-3]); else printf("null");
        printf("}\n");
    }
    return 0;
}

static int order_vectors(void)
{
    static const unsigned long keys[]={0,1,0x7fffffff,0x80000000,0xffffffff,7};
    unsigned long seed,state,i,kind,key,*record;
    fprintf(stderr,"JJFFE order-v1: original F1778/F1780/F1788/F1792/F1793, synthetic marker callbacks; no game primitives or pixels.\n");
    for(seed=1;seed<=128;++seed) {
        order_init();state=seed;
        for(i=0;i<64;++i) {
            state=state*1664525+1013904223;
            kind=(state>>16)%4;
            key=seed%4==0?7:keys[(state>>24)%6];
            if(kind==0) ref_order_group(0,0);
            else if(kind==1) ref_order_group(1,key);
            else {
                record=ref_order_allocate(2,key,0);
                record[0]=(unsigned long)order_marker;record[1]=i;
            }
        }
        ref_order_group(0,0);
        ref_order_flush();
        printf("{\"seed\":%lu,\"active\":%lu,\"state\":%lu,\"order\":[",seed,ref_order_active,ref_order_state);
        for(i=0;i<order_count;++i) printf("%s%lu",i?",":"",order_output[i]);
        printf("]}\n");
    }
    return 0;
}

static unsigned long *composed_marker(unsigned long *record)
{
    if(record<order_buffer || record+3>ref_order_cursor || order_count>=64)
        fail("invalid composed order callback");
    order_output[order_count]=record[1];
    composed_keys[order_count]=record[2];
    order_count++;
    ref_order_state=1;
    return record+3;
}

static int composed_order_vectors(void)
{
    unsigned long instance[0x160/4],vertices[4*9],operands[8]={0},i,key;
    unsigned long *record;
    fprintf(stderr,"JJFFE composed-order-v1: original F1696 depth keys/groups plus F1777/F1778/F1780/F1788 ordering with marker callbacks; synthetic vertices and faces, not the dispatcher or pixels.\n");
    memset(instance,0,sizeof(instance));
    memset(vertices,0,sizeof(vertices));
    instance[0xf0/4]=2;
    instance[0x130/4]=0;
    instance[0x148/4]=0;
    for(i=0;i<4;++i) {
        vertices[i*9+3]=(i<2)?7:79;
        vertices[i*9+5]=2;
    }
    ref_vertex_buffer=vertices;
    order_init();
    /* Group 1: depth vertex 0 (key 0), one face member. */
    ref_depth_command(instance,operands,0x4006);
    key=ref_order_cursor[-3];
    record=ref_order_allocate(3,0,0);
    record[0]=(unsigned long)composed_marker;record[1]=1;record[2]=key;
    /* Group 2: depth vertex 2 (key 9), one face member. */
    ref_depth_command(instance,operands,0x4046);
    key=ref_order_cursor[-3];
    record=ref_order_allocate(3,0,0);
    record[0]=(unsigned long)composed_marker;record[1]=2;record[2]=key;
    /* Close, then a standalone face keyed from vertex 2's view (key 9). */
    ref_depth_command(instance,operands,0x0006);
    key=ref_depth_key(instance,&vertices[2*9+1]);
    record=ref_order_allocate(3,key,0);
    record[0]=(unsigned long)composed_marker;record[1]=3;record[2]=key;
    ref_order_flush();
    printf("{\"order\":[");
    for(i=0;i<order_count;++i) printf("%s%lu",i?",":"",order_output[i]);
    printf("],\"keys\":[");
    for(i=0;i<order_count;++i) printf("%s%lu",i?",":"",composed_keys[i]);
    printf("]}\n");
    return 0;
}

static int execution_vectors(void)
{
#ifndef FFE_COMMAND_PROBE
    fail("--execution requires the separate instrumented jjffe-commands.exe");
    return 2;
#else
    static const unsigned short loop[]={13,0x00c8,7,0x0123,0x0200,0x0402,0x1d,0x01c0,0xff14,0x00c0,0};
    static const unsigned short branch[]={0x8b,0x8002,3,0x123,0x0200,0x0402,0x803c,0xc9,8,0x234,0x0200,0x0604,2,12,31,0x1dd,0xc9c8,3,0x345,0x0402,0x0603,0};
    static const unsigned short paths[]={13,0x00c8,0x805c,0xc9,0x2ed,0xc9c8,0xb3,0x01c0,8,0x456,0x0200,0x0604,2,12,0x8003,3,0x567,0x0200,0x0402,0};
    static const long angles[]={0,1,0x4000,-65537,2147483647};
    static const long depths[]={0,31,32,100,2147483647};
    static const long matrix[]={2147483647,0,0,0,2147483647,0,0,0,2147483647};
    unsigned long instance[0x160/4],globals[0x120/4],buffer[4*8/4],*locals,p,a,b,d,n,i;
    fprintf(stderr,"JJFFE execution-v1: original F1772 dispatcher and command handlers; four primitive-emission probes. Synthetic composed programs, not full stations or pixels.\n");
    for(p=0;p<3;++p) for(a=1;a<=3;++a) for(b=0;b<5;++b) for(d=0;d<5;++d) for(n=0;n<4;++n) {
        memset(instance,0,sizeof(instance));memset(globals,0,sizeof(globals));memset(buffer,0,sizeof(buffer));
        locals=(unsigned long *)((char *)instance+0x54);
        for(i=0;i<8;++i) locals[i]=i*257+7;
        locals[8]=a;locals[9]=(unsigned long)angles[b];
        memcpy((char *)instance+0x9c,matrix,36);
        instance[0xc/4]=(unsigned long)depths[d];instance[0x148/4]=0;
        instance[0x14c/4]=(unsigned long)globals;
        ref_normal_buffer=(unsigned char *)buffer;
        ref_normal_buffer[2*8+3]=(unsigned char)(n&1);
        ref_normal_buffer[3*8+3]=(unsigned char)((n>>1)&1);
        ref_detail_primary=ref_detail_secondary=0;
        execution_probe=1;execution_faces=0;execution_hash=UINT64_C(14695981039346656037);ref_command_alive=1;
        ref_execute(instance,(void *)(p==0?loop:p==1?branch:paths));
        execution_probe=0;
        printf("{\"program\":%lu,\"count\":%lu,\"angle\":%ld,\"depth\":%ld,\"flags\":%lu,\"faces\":%lu,\"emissions\":\"%016llx\",\"locals\":\"%016llx\",\"matrix\":\"%016llx\",\"alive\":%lu}\n",
               p,a,angles[b],depths[d],n,execution_faces,execution_hash,
               fingerprint((unsigned char *)instance+0x54,32),fingerprint((unsigned char *)instance+0x9c,36),ref_command_alive);
    }
    return 0;
#endif
}

static int point_line_vectors(void)
{
#ifndef FFE_COMMAND_PROBE
    fail("--point-line requires the separate instrumented jjffe-commands.exe");
    return 2;
#else
    static const unsigned short program[] = {
        1, 0x0123, 0x4000, 0x0080,
        2, 0x0456, 0x0201,
        0
    };
    unsigned long instance[0x160/4], i;
    memset(instance,0,sizeof(instance));
    point_line_count=0;
    ref_command_alive=1;
    ref_execute(instance,(void *)program);
    fprintf(stderr,"JJFFE point-line-v1: original F1698/F1702 handler argument decomposition; synthetic point/line, not geometry or pixels.\n");
    printf("{\"events\":[");
    for(i=0;i<point_line_count;++i) {
        if(i) printf(",");
        if(point_line_kind[i]==1)
            printf("{\"kind\":\"point\",\"material\":%lu,\"angle\":%ld,\"vertex\":%ld,\"flag\":%ld}",
                   point_line_a[i],(long)(short)point_line_b[i],(long)point_line_c[i],(long)point_line_d[i]);
        else
            printf("{\"kind\":\"line\",\"material\":%lu,\"vertices\":[%ld,%ld]}",
                   point_line_a[i],(long)point_line_b[i],(long)point_line_c[i]);
    }
    printf("]}\n");
    return 0;
#endif
}

static void text_submodel_case(const char *name,const unsigned short *program,int culled)
{
    unsigned long instance[0x160/4],globals[0x120/4],normals[8*8/4],vertices[8*9],i;
    memset(instance,0,sizeof(instance));memset(globals,0,sizeof(globals));
    memset(normals,0,sizeof(normals));memset(vertices,0,sizeof(vertices));
    memset(nested_vertex,0,sizeof(nested_vertex));memset(nested_descriptor,0,sizeof(nested_descriptor));
    instance[0xf0/4]=2;instance[0x14c/4]=(unsigned long)globals;
    ref_vertex_buffer=(long *)vertices;
    ref_normal_buffer=(unsigned char *)normals;
    /* Text culls on its normal's flag; submodels cull when the projected z is behind. */
    if(culled) ref_normal_buffer[2*8+3]=1;
    nested_vertex[3]=culled ? 0 : 1000;
    nested_count=0;point_line_count=0;ref_command_alive=1;
    ref_execute(instance,(void *)program);
    printf("{\"case\":\"%s\",\"events\":[",name);
    for(i=0;i<nested_count;++i)
        printf("%s[%lu,%lu,%lu,%lu]",i?",":"",nested_kind[i],nested_value[i][0],nested_value[i][1],nested_value[i][2]);
    printf("],\"lines\":[");
    for(i=0;i<point_line_count;++i)
        printf("%s[%lu,%lu,%lu]",i?",":"",point_line_a[i],point_line_b[i],point_line_c[i]);
    printf("],\"alive\":%lu}\n",ref_command_alive);
}

static int text_submodel_vectors(void)
{
#ifndef FFE_COMMAND_PROBE
    fail("--text-submodel requires the separate instrumented jjffe-commands.exe");
    return 2;
#else
    static const unsigned short text[]={10,0x0123,0x5a02,0x8104,0x1234,2,0x0456,0x0201,0};
    static const unsigned short submodel[]={0x1c6e,0x0305,2,0x0456,0x0201,0};
    static const unsigned short extended[]={0x002e,0x8306,0x0201,0x0403,2,0x0456,0x0201,0};
    static const unsigned short stopped[]={0x002e,0x8306,0x7f01,0x0403,2,0x0456,0x0201,0};
    fprintf(stderr,"JJFFE text-submodel-v1: original F1747/F1749/F1750 operand decoding and cursor advance with nested calls recorded, not glyph/child execution or pixels.\n");
    text_submodel_case("text-visible",text,0);
    text_submodel_case("text-culled",text,1);
    text_submodel_case("submodel-visible",submodel,0);
    text_submodel_case("submodel-culled",submodel,1);
    text_submodel_case("extended-visible",extended,0);
    text_submodel_case("extended-culled",extended,1);
    text_submodel_case("stopped-visible",stopped,0);
    return 0;
#endif
}

static int colour_vectors(void)
{
#ifndef FFE_COMMAND_PROBE
    fail("--colour requires the separate instrumented jjffe-commands.exe");
    return 2;
#else
    static const unsigned short lit[]={26,0x0123,0x0002,2,0x0456,0x0201,0};
    static const unsigned short local[]={26,0x1abc,0x0001,2,0x0456,0x0201,0};
    static const unsigned short unlit[]={26,0x3abc,0x0003,2,0x0456,0x0201,0};
    static const unsigned short textured[]={26,0x5005,0x0002,2,0x0456,0x0201,0};
    static const unsigned short selected[]={26,0x1111,0xc802,0x0222,0x0333,0x0444,0x0555,0x0666,0x0777,0x1888,2,0x0456,0x0201,0};
    static const unsigned short normal_zero[]={26,0x1123,0x0082,26,0x0010,0x0000,2,0x0456,0x0201,0};
    static const unsigned short wrapped[]={26,0x1fff,0x0007,2,0x0456,0x0201,0};
    static const unsigned short *programs[]={lit,local,unlit,textured,selected,normal_zero,wrapped};
    static const unsigned char colours[][3]={{0,0,0},{5,9,250}};
    unsigned long instance[0x160/4],globals[0x120/4],normals[8*8/4],p,c,l,i;
    unsigned char *bytes;
    fprintf(stderr,"JJFFE colour-v1: original F1762 colour command through F1772 with F1692/F1693/F1691; untextured normal-0 writes only, synthetic normals, not face shading or pixels.\n");
    for(p=0;p<7;++p) for(c=0;c<2;++c) for(l=0;l<(p==4?10u:1u);++l) {
        memset(instance,0,sizeof(instance));memset(globals,0,sizeof(globals));memset(normals,0,sizeof(normals));
        instance[0x14c/4]=(unsigned long)globals;
        ((long *)((char *)instance+0x54))[8]=(long)l;
        bytes=(unsigned char *)instance+0xf4;
        bytes[0]=colours[c][0];bytes[1]=colours[c][1];bytes[2]=colours[c][2];
        ref_normal_buffer=(unsigned char *)normals;
        for(i=0;i<8;++i) {
            ref_normal_buffer[i*8]=(unsigned char)(i*40+7);
            ref_normal_buffer[i*8+1]=(unsigned char)(i*3);
            ref_normal_buffer[i*8+2]=(unsigned char)(255-i);
        }
        point_line_count=0;ref_command_alive=1;
        ref_execute(instance,(void *)programs[p]);
        printf("{\"program\":%lu,\"instance\":%lu,\"local\":%lu,\"colour\":[%u,%u,%u],\"normal0\":[%u,%u,%u],\"lines\":[",
               p,c,l,bytes[0],bytes[1],bytes[2],ref_normal_buffer[0],ref_normal_buffer[1],ref_normal_buffer[2]);
        for(i=0;i<point_line_count;++i)
            printf("%s[%lu,%lu,%lu]",i?",":"",point_line_a[i],point_line_b[i],point_line_c[i]);
        printf("],\"alive\":%lu}\n",ref_command_alive);
    }
    return 0;
#endif
}

static int station_vectors(void)
{
#ifndef FFE_COMMAND_PROBE
    fail("--station requires the separate instrumented jjffe-commands.exe");
    return 2;
#else
    static const int models[]={74,75};
    static unsigned char normals[32768*8];
    static unsigned long instance[0x160/4],globals[0x120/4],slots[128*9];
    static const long identity[9]={2147483647,0,0,0,2147483647,0,0,0,2147483647};
    unsigned long m,i;
    unsigned char *colour;
    fprintf(stderr,"JJFFE station-v1: real station command streams through original F1772 with real depth/colour/transform/vertex handlers; face/point/line emitters and text/child internals recorded, not executed. Synthetic instance, not pixels or render order.\n");
    for(m=0;m<2;++m) {
        unsigned long *model=ref_model(models[m]);
        memset(instance,0,sizeof(instance));memset(globals,0,sizeof(globals));
        memset(slots,0,sizeof(slots));memset(normals,0,sizeof(normals));
        memset(nested_descriptor,0,sizeof(nested_descriptor));
        memcpy((char *)instance+0x14,identity,36);
        memcpy((char *)instance+0x9c,identity,36);
        instance[0xc/4]=100;
        instance[0xf0/4]=2;
        instance[0x14c/4]=(unsigned long)globals;
        instance[0x150/4]=model[1];
        ref_vertex_buffer=(long *)slots;
        ref_normal_buffer=normals;
        ref_detail_primary=ref_detail_secondary=0;
        nested_descriptor[0x1c/4]=-0x40000000L;
        order_init();
        nested_station=1;nested_count=0;point_line_count=0;
        execution_probe=1;execution_faces=0;execution_hash=UINT64_C(14695981039346656037);
        ref_command_alive=1;
        ref_execute(instance,(void *)model[0]);
        execution_probe=0;nested_station=0;
        colour=(unsigned char *)instance+0xf4;
        printf("{\"model\":%d,\"faces\":%lu,\"emissions\":\"%016llx\",\"points\":[",models[m],execution_faces,(unsigned long long)execution_hash);
        for(i=0;i<point_line_count;++i) {
            if(point_line_kind[i]==1) printf("%s[1,%lu,%ld,%lu,%lu]",i?",":"",point_line_a[i]&0xffff,(long)(short)point_line_b[i],point_line_c[i],point_line_d[i]);
            else printf("%s[2,%lu,%lu,%lu,0]",i?",":"",point_line_a[i]&0xffff,point_line_b[i],point_line_c[i]);
        }
        printf("],\"nested\":[");
        for(i=0;i<nested_count;++i)
            printf("%s[%lu,%lu,%lu,%lu]",i?",":"",nested_kind[i],nested_value[i][0],nested_value[i][1],nested_value[i][2]);
        printf("],\"locals\":\"%016llx\",\"basis\":[",(unsigned long long)fingerprint((unsigned char *)instance+0x54,32));
        for(i=0;i<9;++i) printf("%s%ld",i?",":"",(long)instance[0x9c/4+i]);
        printf("],\"colour\":[%u,%u,%u],\"handedness\":%lu,\"alive\":%lu}\n",colour[0],colour[1],colour[2],instance[0xfc/4],ref_command_alive);
    }
    return 0;
#endif
}

static int station_queue_vectors(void)
{
#ifndef FFE_COMMAND_PROBE
    fail("--station-queue requires the separate instrumented jjffe-commands.exe");
    return 2;
#else
    static const int models[]={74,75};
    static unsigned char normals[32768*8];
    static unsigned long instance[0x160/4],globals[0x120/4],slots[128*9];
    static const long identity[9]={2147483647,0,0,0,2147483647,0,0,0,2147483647};
    static const long positions[][3]={{0,0,100},{0,0,600},{150,-80,400},{-40,30,250},{-260,0,300},{0,10,40}};
    static const long frames[][9]={
        {2147483647,0,0,0,2147483647,0,0,0,2147483647},
        {2147483647,0,0,0,2147483647,0,0,0,2147483647},
        {2147483647,0,0,0,2147483647,0,0,0,2147483647},
        {1518500249,0,-1518500249,0,2147483647,0,1518500249,0,1518500249},
        {2147483647,0,0,0,2147483647,0,0,0,2147483647},
        {2147483647,0,0,0,2147483647,0,0,0,2147483647}
    };
    unsigned long m,v,i;
    static unsigned long queue_buffer[65536];
    fprintf(stderr,"JJFFE station-queue-v1: real station streams through original F1772 with faces drawn by the real F1708/F1712; every face queue allocation and depth-group change logged in order. Points/lines/text/children not queued; no flush or pixels.\n");
    for(m=0;m<2;++m) for(v=0;v<6;++v) {
        unsigned long *model=ref_model(models[m]);
        memset(instance,0,sizeof(instance));memset(globals,0,sizeof(globals));
        memset(slots,0,sizeof(slots));memset(normals,0,sizeof(normals));
        memset(nested_descriptor,0,sizeof(nested_descriptor));
        memcpy((char *)instance+0x14,frames[v],36);
        memcpy((char *)instance+0x9c,identity,36);
        memcpy((char *)instance+4,positions[v],12);
        instance[0xf0/4]=2;
        instance[0x14c/4]=(unsigned long)globals;
        instance[0x150/4]=model[1];
        ref_vertex_buffer=(long *)slots;
        ref_normal_buffer=normals;
        ref_detail_primary=ref_detail_secondary=0;
        nested_descriptor[0x1c/4]=-0x40000000L;
        memset(queue_buffer,0,sizeof(queue_buffer));
        ref_order_base=queue_buffer;ref_order_active=0;ref_order_state=0;
        ref_order_init(queue_buffer);
        queue_mode=1;queue_count=0;
        nested_station=1;nested_count=0;point_line_count=0;

        ref_command_alive=1;
        ref_execute(instance,(void *)model[0]);
        queue_mode=0;nested_station=0;
        printf("{\"model\":%d,\"view\":%lu,\"queue\":[",models[m],v);
        for(i=0;i<queue_count;++i) printf("%s[%lu,%lu,%lu]",i?",":"",queue_log[i][0],queue_log[i][1],queue_log[i][2]);
        printf("],\"alive\":%lu}\n",ref_command_alive);
    }
    return 0;
#endif
}

static void visibility_case(unsigned long *instance,unsigned long number,short operand)
{
    static const unsigned short codes[]={11,12,0x8b,0x8c,0xff8b,0xff8c};
    unsigned char operands[128],*result;
    unsigned long i;
    memset(operands,0,sizeof(operands));memcpy(operands+64,&operand,2);
    printf("{\"case\":%lu,\"operand\":%d,\"depth\":%ld,\"scale\":%ld,\"detail\":[%ld,%ld],\"predicate\":%ld,\"branches\":[",
           number,operand,(long)instance[0xc/4],(long)instance[0x148/4],
           ref_detail_primary,ref_detail_secondary,ref_visibility(instance,operand));
    for(i=0;i<6;++i) {
        ref_command_alive=1;
        result=(codes[i]&31)==11 ? ref_visibility_branch(instance,operands+64,codes[i]) : ref_visibility_inverse(instance,operands+64,codes[i]);
        if(i) printf(",");
        if(!ref_command_alive) printf("null");
        else printf("%ld",(long)(result-(operands+64))+2);
    }
    printf("]}\n");
}

static int visibility_vectors(void)
{
    static const long depths[]={(-2147483647-1),-1,0,1,100,2147483647};
    static const long scales[]={-33,-1,0,1,15,31,32,33,2147483647,(-2147483647-1)};
    static const long details[][2]={{0,0},{2,999},{1,-2},{3,31}};
    static const short operands[]={0,1,31,32767};
    unsigned long instance[0x160/4],normal_buffer[6*8/4],i,d,s,t,number=0;
    memset(instance,0,sizeof(instance));memset(normal_buffer,0,sizeof(normal_buffer));
    ref_normal_buffer=(unsigned char *)normal_buffer;
    for(i=0;i<6;++i) ref_normal_buffer[i*8+3]=(unsigned char)(i&1);
    fprintf(stderr,"JJFFE visibility-v1: original F1751/F1752/F1755 predicates and branches, evaluated synthetic normals, not complete command execution.\n");
    for(i=0;i<4;++i) for(d=0;d<6;++d) for(s=0;s<10;++s) for(t=0;t<4;++t) {
        instance[0xc/4]=(unsigned long)depths[d];instance[0x148/4]=(unsigned long)scales[s];
        ref_detail_primary=details[t][0];ref_detail_secondary=details[t][1];
        visibility_case(instance,number++,operands[i]);
    }
    for(i=0;i<6;++i) visibility_case(instance,number++,(short)(0x8000|i));
    return 0;
}

static int rotation_operand_vectors(void)
{
    static const unsigned short operands[]={0,1,0x3f,0x40,0x7f,0x1234,0x3fff,0x4000,0x7fff,0x8000,0xffff,0x80,0xbf,0xc8,0xc9};
    static const long matrices[][9]={
        {2147483647,0,0,0,2147483647,0,0,0,2147483647},
        {(-2147483647-1),2147483647,-1,1,-65536,32768,123456789,-987654321,0},
        {123,456,789,-1011,1213,-1415,1617,-1819,2021}
    };
    unsigned long instance[0x160/4],globals[0x120/4],*locals,i,m,axis,code;
    memset(instance,0,sizeof(instance));memset(globals,0,sizeof(globals));
    locals=(unsigned long *)((char *)instance+0x54);
    for(i=0;i<64;++i) {
        locals[i]=(i*1234567)^0x5a5a5a5a;
        ((unsigned short *)((char *)globals+0x9c))[i]=(unsigned short)(i*257+19);
    }
    fprintf(stderr,"JJFFE rotation-operands-v1: original F1474 and F1742/F1743, not full transform dispatch or model execution.\n");
    for(i=0;i<65536;++i) {
        /* Local slot 62 aliases the instance's global pointer; no global read
           occurs in that selector, so restore its synthetic value for the call. */
        instance[0x14c/4]=((i&255)==254) ? ((62UL*1234567)^0x5a5a5a5a) : (unsigned long)globals;
        printf("{\"kind\":\"operand\",\"encoded\":%lu,\"value\":%ld}\n",i,ref_angle(instance,(short)i));
    }
    for(m=0;m<3;++m) for(axis=0;axis<3;++axis) for(i=0;i<15;++i) {
        memset(instance,0,sizeof(instance));
        instance[0x14c/4]=(unsigned long)globals;
        locals[8]=(unsigned long)-65537;locals[9]=2147483647;
        memcpy((char *)instance+0x9c,matrices[m],36);
        code=0x8000|(axis<<5)|28;
        if(ref_rotate_command(instance,(void *)(operands+i),code)!=(unsigned char *)(operands+i)+2)
            fail("rotation command cursor differs");
        printf("{\"kind\":\"command\",\"matrix\":%lu,\"axis\":%lu,\"encoded\":%u,\"hash\":\"%016llx\"}\n",
               m,axis,operands[i],fingerprint((unsigned char *)instance+0x9c,36));
    }
    return 0;
}

static int base_transform_vectors(void)
{
    static const long bases[][9]={
        {2147483647,0,0,0,2147483647,0,0,0,2147483647},
        {123456789,-987654321,5,-6,7,(-2147483647-1),2147483647,-1,65536}
    };
    static const unsigned long hands[]={0,0xffffffffUL,0x12345678UL};
    static const unsigned short operands[]={0,0x1234,0xffff};
    unsigned long instance[0x160/4],globals[0x120/4],*locals,b,h,form,sel,neg,axis,o,i,code;
    unsigned short operand;
    unsigned char *next;
    memset(globals,0,sizeof(globals));
    for(i=0;i<64;++i) ((unsigned short *)((char *)globals+0x9c))[i]=(unsigned short)(i*257+19);
    fprintf(stderr,"JJFFE base-transform-v1: original F1741 base-relative forms (F1739 row permutation/negation/handedness, then F1743), not vertex-defined bases or model execution.\n");
    for(b=0;b<2;++b) for(h=0;h<3;++h) for(form=0;form<2;++form) for(sel=0;sel<7;++sel)
    for(neg=0;neg<8;++neg) for(axis=0;axis<3;++axis) for(o=0;o<3;++o) {
        memset(instance,0,sizeof(instance));
        instance[0x14c/4]=(unsigned long)globals;
        locals=(unsigned long *)((char *)instance+0x54);
        for(i=0;i<8;++i) locals[i]=i*0x01010101UL;
        for(i=0;i<9;++i) instance[0x9c/4+i]=0xdead0000UL+i;
        memcpy((char *)instance+0x14,bases[b],36);
        instance[0xf8/4]=hands[h];
        code=(form<<14)|(neg<<10)|(sel<<7)|(axis<<5)|28;
        operand=operands[o];
        next=ref_transform_command(instance,&operand,code);
        printf("{\"base\":%lu,\"hand\":%lu,\"code\":%lu,\"operand\":%u,\"advance\":%ld,\"matrix\":[",
               b,h,code,operand,(long)(next-(unsigned char *)&operand));
        for(i=0;i<9;++i) printf("%s%ld",i?",":"",(long)instance[0x9c/4+i]);
        printf("],\"handedness\":%lu}\n",instance[0xfc/4]);
    }
    return 0;
}

static int transform_vectors(void)
{
    static const long angles[]={0,1,0x3fff,0x4000,0x4001,0x7fff,0x8000,0xbfff,0xc000,0xffff,-1,-65536,65536,(-2147483647-1),2147483647};
    static const long matrices[][9]={
        {2147483647,0,0,0,2147483647,0,0,0,2147483647},
        {(-2147483647-1),2147483647,-1,1,-65536,32768,123456789,-987654321,0},
        {123,456,789,-1011,1213,-1415,1617,-1819,2021}
    };
    static const long amplitudes[]={(-2147483647-1),-65536,-1,0,1,65535,2147483647};
    unsigned long instance[0x160/4],i,m,axis,op,a;
    unsigned char operands[2]={0xc8,0xc9};
    fprintf(stderr,"JJFFE transforms-v1: original Sin32/F1744/F1758, not full model execution or pixels.\n");
    for(i=0;i<65536;++i)
        printf("{\"kind\":\"sine\",\"angle\":%lu,\"value\":%ld}\n",i,ref_sin32((long)i));
    for(i=10;i<15;++i)
        printf("{\"kind\":\"sine\",\"angle\":%ld,\"value\":%ld}\n",angles[i],ref_sin32(angles[i]));
    for(m=0;m<3;++m) for(axis=0;axis<3;++axis) for(i=0;i<15;++i) {
        memset(instance,0,sizeof(instance));
        memcpy((char *)instance+0x9c,matrices[m],36);
        ref_rotate(instance,axis,ref_sin32((long)((unsigned long)angles[i]+0x4000)),ref_sin32(angles[i]));
        printf("{\"kind\":\"rotate\",\"matrix\":%lu,\"axis\":%lu,\"angle\":%ld,\"hash\":\"%016llx\"}\n",
               m,axis,angles[i],fingerprint((unsigned char *)instance+0x9c,36));
    }
    for(op=13;op<=14;++op) for(a=0;a<7;++a) for(i=0;i<15;++i) {
        unsigned long *locals=(unsigned long *)((char *)instance+0x54);
        memset(instance,0,sizeof(instance));locals[8]=amplitudes[a];locals[9]=angles[i];
        if(ref_command_math(instance,operands,(op<<4)|13)!=operands+2) fail("trig cursor differs");
        printf("{\"kind\":\"math\",\"op\":%lu,\"a\":%ld,\"angle\":%ld,\"value\":%lu}\n",
               op,amplitudes[a],angles[i],locals[0]);
    }
    return 0;
}

static int command_vectors(void)
{
#ifndef FFE_COMMAND_PROBE
    fail("--commands requires the separate instrumented jjffe-commands.exe, not the unmodified smoke oracle");
    return 2;
#else
    static const long values[] = {(-2147483647-1),-65536,-1,0,1,31,32,63,65535,2147483647};
    static const unsigned short branches[] = {19,20,0x93,0x94,0xffd3,0xffd4};
    static const unsigned char bits[] = {0,1,2,32,33,255};
    unsigned long instance[0x160/4], globals[0x120/4];
    unsigned char operands[128];
    static unsigned char materials[65536*16];
    unsigned long operation,a,b,destination,i,j,code,record[4],*locals;
    unsigned char *result;
    memset(instance,0,sizeof(instance));
    memset(globals,0,sizeof(globals));
    locals=(unsigned long *)((char *)instance+0x54);
    instance[0x14c/4]=(unsigned long)globals;
    for (i=0;i<64;++i) ((unsigned short *)((char *)globals+0x9c))[i]=(unsigned short)(i*257+19);
    fprintf(stderr,"JJFFE commands-v1: original math/bit-branch/material routines; original triangle/quad handlers with four emission-call probes. Not full model execution or pixels.\n");
    for (operation=0;operation<16;++operation) {
        if (operation==13 || operation==14) continue;
        for (a=0;a<10;++a) for (b=0;b<10;++b) {
            long av=values[a],bv=values[b];
            if (operation==3 && av==(-2147483647-1) && bv==-1) continue;
            if (operation==8 && bv==0) continue;
            if (operation==10 && ((unsigned long)av+(unsigned long)bv)>63) continue;
            destination=(a+b)%8;
            code=(destination<<8)|(operation<<4)|13;
            locals[8]=(unsigned long)av; locals[9]=(unsigned long)bv;
            operands[64]=0xc8; operands[65]=0xc9;
            if (ref_command_math(instance,operands+64,code)!=operands+66) fail("math cursor differs");
            printf("{\"kind\":\"math\",\"op\":%lu,\"a\":%ld,\"b\":%ld,\"destination\":%lu,\"result\":%lu}\n",
                operation,av,bv,destination,locals[destination]);
        }
    }
    for (i=0;i<6;++i) for (a=0;a<10;++a) for (b=0;b<6;++b) {
        code=branches[i]; locals[8]=(unsigned long)values[a];
        operands[64]=0xc8; operands[65]=bits[b];
        ref_command_alive=1;
        result=(code&31)==19 ? ref_command_clear(instance,operands+64,code) : ref_command_set(instance,operands+64,code);
        printf("{\"kind\":\"branch\",\"code\":%lu,\"a\":%ld,\"bit\":%u,\"relative\":%ld}\n",
            code,values[a],bits[b],ref_command_alive ? (long)(result-(operands+62)) : -999L);
    }
    for (i=0;i<65536;++i) {
        ref_material(materials+i*16,(short)i);
        materials[i*16+3]=0; /* Unused colour-structure padding is not a semantic result. */
    }
    printf("{\"kind\":\"material\",\"hash\":\"%016llx\"}\n",(unsigned long long)fingerprint(materials,sizeof(materials)));
    for (i=0;i<4;++i) for (j=0;j<8;++j) {
        static const unsigned short codes[]={3,4,7,8};
        unsigned short *words=(unsigned short *)record;
        unsigned char *payload=(unsigned char *)record;
        void *end;
        code=codes[i]; memset(record,0,sizeof(record)); words[0]=(unsigned short)(0x1023+j*0x202);
        payload[2]=(unsigned char)(j*2); payload[3]=(unsigned char)(j*2+2);
        payload[4]=(unsigned char)(j*2+4); payload[5]=(unsigned char)(j*2+6); words[3]=(unsigned short)(j*2);
        probe_face_count=0;
        if (code==3) end=ref_triangle(instance,record,code);
        else if (code==4) end=ref_quad(instance,record,code);
        else if (code==7) end=ref_mirror_triangle(instance,record,code);
        else end=ref_mirror_quad(instance,record,code);
        if ((unsigned char *)end!=payload+((code==3 || code==7)?6:8)) fail("face cursor differs");
        printf("{\"kind\":\"face\",\"code\":%lu,\"seed\":%lu,\"faces\":[",code,j);
        for (a=0;a<(unsigned long)probe_face_count;++a) {
            printf("%s[%lu,%lu,%lu,%lu,%lu,%lu]",a?",":"",probe_faces[a][0],probe_faces[a][1],
                probe_faces[a][2],probe_faces[a][3],probe_faces[a][4],probe_faces[a][5]);
        }
        printf("]}\n");
    }
    if (fflush(stdout)) fail("cannot flush command vectors");
    return 0;
#endif
}

int main(int argc, char **argv)
{
    const char *count;
    if (argc == 2 && !strcmp(argv[1], "--normals")) return normal_vectors();
    if (argc == 2 && !strcmp(argv[1], "--transforms")) return transform_vectors();
    if (argc == 2 && !strcmp(argv[1], "--rotation-operands")) return rotation_operand_vectors();
    if (argc == 2 && !strcmp(argv[1], "--visibility")) return visibility_vectors();
    if (argc == 2 && !strcmp(argv[1], "--execution")) return execution_vectors();
    if (argc == 2 && !strcmp(argv[1], "--point-line")) return point_line_vectors();
    if (argc == 2 && !strcmp(argv[1], "--text-submodel")) return text_submodel_vectors();
    if (argc == 2 && !strcmp(argv[1], "--colour")) return colour_vectors();
    if (argc == 2 && !strcmp(argv[1], "--station")) return station_vectors();
    if (argc == 2 && !strcmp(argv[1], "--station-queue")) return station_queue_vectors();
    if (argc == 2 && !strcmp(argv[1], "--base-transform")) return base_transform_vectors();
    if (argc == 2 && !strcmp(argv[1], "--depth")) return depth_vectors();
    if (argc == 2 && !strcmp(argv[1], "--depth-commands")) return depth_command_vectors();
    if (argc == 2 && !strcmp(argv[1], "--order")) return order_vectors();
    if (argc == 2 && !strcmp(argv[1], "--composed-order")) return composed_order_vectors();
    if (argc == 2 && !strcmp(argv[1], "--projection")) return projection_vectors();
    if (argc == 2 && !strcmp(argv[1], "--live-vertices")) return live_vertex_vectors();
    if (argc == 2 && !strcmp(argv[1], "--commands")) return command_vectors();
    if (argc == 2 && !strcmp(argv[1], "--vectors")) return vectors();
    if (argc == 2 && !strcmp(argv[1], "--font")) return font_vectors();
    if (argc == 2 && !strcmp(argv[1], "--blit")) return blit_vectors();
    if (argc == 2 && !strcmp(argv[1], "--models")) return model_vectors();
    state_mode = argc == 3 && !strcmp(argv[1], "--state");
    if (argc != 2 && !state_mode)
        fail("usage: jjffe-smoke <frames> | --state <frames> | --vectors | --font | --blit | --models");
    count = argv[state_mode ? 2 : 1];
    {
        char *end;
        limit = strtoul(count, &end, 10);
        if (*end || limit < 3 || limit > 10000) fail("invalid frame count");
    }
    if (state_mode) fprintf(stderr, "bounded-new-game-v1: Y then intro choice 1; synthetic clock, selected numeric state only, no flight/save validation.\n");
    asmmain(0, NULL);
    fail("JJFFE returned before the requested frame bound");
    return 2;
}
