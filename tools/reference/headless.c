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
extern unsigned long ref_jitter_a, ref_jitter_b;
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
static unsigned long queue_count, queue_log[8192][3];
static void queue_event(unsigned long kind, unsigned long active, unsigned long key)
{
    if (queue_count >= 8192) { fprintf(stderr,"too many queue events\n"); exit(2); }
    queue_log[queue_count][0]=kind; queue_log[queue_count][1]=active; queue_log[queue_count][2]=key;
    queue_count++;
}
void trace_group(long active, unsigned long key)
{
    if (queue_mode) queue_event(2,active!=0,active ? key : 0);
    ref_order_group(active,key);
}
/* Texture-queue mode: the last face allocation and the F1691 flat colour of the
   face being emitted, captured around each real F1708/F1712 call. */
static int texture_queue_mode;
static unsigned long *face_entry;
static int face_colour_called;
static unsigned char face_colour[3];
extern void *ref_face_colour(void *output, long rgb, long texture, long unlit, long local);
void *trace_face_colour(void *output, long rgb, long texture, long unlit, long local)
{
    void *result = ref_face_colour(output,rgb,texture,unlit,local);
    face_colour_called=1;
    memcpy(face_colour,output,3);
    return result;
}
unsigned long *trace_alloc(long words, unsigned long key, long force)
{
    if (queue_mode) queue_event(1,0,key);
    face_entry = ref_order_allocate(words,key,force);
    return face_entry;
}
/* F1721's own entry (its F1794 word); the segment handlers append after it. */
static unsigned long *polygon_entry;
static void texture_face_begin(void);
static void texture_face_end(int kind, long material, unsigned long normal);
unsigned long *trace_polygon_alloc(long words, unsigned long key, long force)
{
    if (queue_mode) queue_event(3,0,key);
    polygon_entry=ref_order_allocate(words,key,force);
    return polygon_entry;
}
static unsigned long probe_faces[2][6];
/* Child mode (--child-model): children really execute; every face is logged with its
   nesting depth and then queued by the real F1708/F1712. */
static int child_mode, child_depth;
static void nested_event(unsigned long kind, unsigned long a, unsigned long b, unsigned long c);
static void child_face(unsigned long material, unsigned long normal, unsigned long a, unsigned long b, unsigned long c, unsigned long d)
{
    nested_event(22,(unsigned long)child_depth,(material&0xffff)|(normal&0xffff)<<16,
                 (a&0xff)|(b&0xff)<<8|(c&0xff)<<16|(d&0xff)<<24);
}
static int execution_probe;
static unsigned long execution_faces;
static uint64_t execution_hash;
static void execution_emission(void *instance,const unsigned long *face);
int trace_triangle(void *instance, long material, unsigned long a, unsigned long b, unsigned long c, unsigned long normal)
{
    unsigned long *face;
    if(child_mode) { child_face((unsigned long)material,normal,a,b,c,255); return ref_face3(instance,material,a,b,c,normal); }
    if(queue_mode) {
        int result;
        texture_face_begin();
        result=ref_face3(instance,material,a,b,c,normal);
        texture_face_end(3,material,normal);
        return result;
    }
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
    if(child_mode) { child_face((unsigned long)material,normal,a,b,c,d); return ref_face4(instance,material,a,b,c,d,normal); }
    if(queue_mode) {
        int result;
        texture_face_begin();
        result=ref_face4(instance,material,a,b,c,d,normal);
        texture_face_end(4,material,normal);
        return result;
    }
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

static unsigned long point_line_count, point_line_kind[4096], point_line_a[4096], point_line_b[4096], point_line_c[4096], point_line_d[4096];
void trace_point_args(void *instance, long material, long angle, long vertex, long flag)
{
    (void)instance;
    if (point_line_count >= 4096) { fprintf(stderr,"too many point callbacks\n"); exit(2); }
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
    if (point_line_count >= 4096) { fprintf(stderr,"too many line callbacks\n"); exit(2); }
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
#define NESTED_LIMIT 8192
static unsigned long nested_count, nested_kind[NESTED_LIMIT], nested_value[NESTED_LIMIT][3];
static long nested_vertex[9], nested_descriptor[16];
/* Station mode: vertex hooks resolve through the real F1470/F1471, text skips its
   glyphs (F1745 reports no room) and child descriptors always cull (negative radius). */
static int nested_station;
static void nested_event(unsigned long kind, unsigned long a, unsigned long b, unsigned long c)
{
    if (nested_count >= NESTED_LIMIT) { fprintf(stderr,"too many nested probe callbacks\n"); exit(2); }
    nested_kind[nested_count]=kind;
    nested_value[nested_count][0]=a; nested_value[nested_count][1]=b; nested_value[nested_count][2]=c;
    nested_count++;
}
extern unsigned long ref_depth_key(void *instance,const long *point);
unsigned long trace_polygon_key(void *instance, const long *view)
{
    unsigned long key=ref_depth_key(instance,view);
    nested_event(11,key,0,0);
    return key;
}
/* F1721 (opcode 5): when F1326 converts the fill colour, the polygon's queue holds
   F1794, the segment handlers' entries, then the F1799 fill just allocated. Each
   entry becomes event 19 [kind, a, b] (1 F1795 edge a-b, 2 F1796 edge to a,
   3 F1797 curve a, b) plus 20 [c, d] for a curve's last two points; points are the
   stored screen dwords (x | y << 16). Event 21 closes with the colour and entry
   count; the palette value is not modelled. An abandoned polygon (F1779, then
   F1813) logs 22 instead. */
extern void ref_poly_start(void), ref_poly_edge(void), ref_poly_to(void), ref_poly_curve(void), ref_poly_fill(void);
unsigned long trace_polygon_colour(unsigned long colour)
{
    unsigned long *word=polygon_entry, *end=ref_order_cursor-2, entries=0;
    if (!word || word[0]!=(unsigned long)ref_poly_start || end[0]!=(unsigned long)ref_poly_fill) {
        fprintf(stderr,"unexpected polygon queue layout\n"); exit(2);
    }
    for (++word; word<end; ++entries) {
        if (word[0]==(unsigned long)ref_poly_edge) { nested_event(30,1,word[1],word[2]); word+=3; }
        else if (word[0]==(unsigned long)ref_poly_to) { nested_event(30,2,word[1],0); word+=2; }
        else if (word[0]==(unsigned long)ref_poly_curve) {
            nested_event(30,3,word[1],word[2]); nested_event(31,word[3],word[4],0); word+=5;
        } else { fprintf(stderr,"unknown polygon queue entry\n"); exit(2); }
    }
    if (word!=end) { fprintf(stderr,"polygon queue entries overrun the fill\n"); exit(2); }
    nested_event(32,colour&0xffffff,entries,0);
    return 0;
}
void trace_polygon_abandon(unsigned long *entry)
{
    nested_event(33,entry==polygon_entry,0,0);
    ref_order_cursor=entry;
}
/* F1759 (opcode 15): the sample and volume it would play; no sound in the adapter. */
void trace_sound(long sample, long volume) { nested_event(18,(unsigned long)sample,(unsigned long)volume,0); }
/* F1764 (opcode 18 labels): key with the projected screen dword just before the view. */
unsigned long trace_label_key(void *instance, const long *view)
{
    unsigned long key=ref_depth_key(instance,view);
    nested_event(17,key,(unsigned long)view[-1],0);
    return key;
}
unsigned long *trace_label_alloc(long words, unsigned long key, long force)
{
    if (queue_mode) queue_event(6,0,key);
    return ref_order_allocate(words,key,force);
}
/* F1737 (opcode 22): key, allocation, then the filled entry when F1326 converts its
   colour. Screen words are packed x | y << 16; the palette value is not modelled. */
static unsigned long *spline_entry;
unsigned long trace_spline_key(void *instance, const long *view)
{
    unsigned long key=ref_depth_key(instance,view);
    nested_event(14,key,0,0);
    return key;
}
unsigned long *trace_spline_alloc(long words, unsigned long key, long force)
{
    if (queue_mode) queue_event(5,0,key);
    spline_entry=ref_order_allocate(words,key,force);
    return spline_entry;
}
#define SPLINE_POINT(i) ((spline_entry[1+2*(i)]&0xffff)|(spline_entry[2+2*(i)]<<16))
unsigned long trace_spline_colour(unsigned long colour)
{
    nested_event(15,SPLINE_POINT(0),SPLINE_POINT(1),SPLINE_POINT(2));
    nested_event(16,SPLINE_POINT(3),colour&0xffffff,0);
    return 0;
}
/* F1738 (opcode 9) thruster glows. Each F1805 entry logs its key (41) from F1777;
   F1778 is logged as queue event 7. When the sixth F1326 of the entry's colour ramp
   runs, the entry holds its four screen dwords (x | y << 16) and its texture word:
   42 [p0, p1, p2], 43 [p3, texture, 0], then the six ramp colours as F1326 receives
   them (r | g << 8 | b << 16): 44 [c1, c2, c3], 45 [c4, c5, c6]. Word 6 (slot 0) is
   never written by F1738. The returned value is F1326's packed colour; its
   upper-palette request flag (DATA_009139) is not modelled. */
static unsigned long *thruster_entry;
static unsigned long thruster_ramp[6];
static int thruster_slot;
static unsigned long pack_colour(unsigned long colour)
{
    return ((colour&0xff)<<8)+(((colour>>8)&0xff)<<4)+((colour>>16)&0xff) & 0xffff;
}
unsigned long trace_thruster_key(void *instance, const long *view)
{
    unsigned long key=ref_depth_key(instance,view);
    nested_event(41,key,0,0);
    return key;
}
unsigned long *trace_thruster_alloc(long words, unsigned long key, long force)
{
    if (queue_mode) queue_event(7,0,key);
    thruster_entry=ref_order_allocate(words,key,force);
    thruster_slot=0;
    return thruster_entry;
}
unsigned long trace_thruster_colour(unsigned long colour)
{
    if (thruster_slot>=6) { fprintf(stderr,"too many thruster ramp colours\n"); exit(2); }
    thruster_ramp[thruster_slot++]=colour&0xffffff;
    if (thruster_slot==6) {
        nested_event(42,thruster_entry[1],thruster_entry[2],thruster_entry[3]);
        nested_event(43,thruster_entry[4],thruster_entry[5],0);
        nested_event(44,thruster_ramp[0],thruster_ramp[1],thruster_ramp[2]);
        nested_event(45,thruster_ramp[3],thruster_ramp[4],thruster_ramp[5]);
    }
    return pack_colour(colour);
}
/* A thruster of width 1..4 (or negative) is a line: F1738 calls F1704 with the two
   resolved entries. 46 [A screen, B screen, colour] records the call; F1704 then
   queues (queue event 8, nested 47 [key]) and fills its F1803 entry: 48 [x0 | y0 << 16,
   x1 | y1 << 16] with the stored x (x - 1, or 0) and y dwords. */
extern void ref_line_queue(void *instance, long *a, long *b, unsigned long colour);
static unsigned long *line_entry;
void trace_thruster_line(void *instance, long *a, long *b, unsigned long colour)
{
    nested_event(46,(unsigned long)a[0],(unsigned long)b[0],colour&0xffffff);
    ref_line_queue(instance,a,b,colour);
}
unsigned long *trace_line_alloc(long words, unsigned long key, long force)
{
    if (queue_mode) queue_event(8,0,key);
    nested_event(47,key,0,0);
    line_entry=ref_order_allocate(words,key,force);
    return line_entry;
}
unsigned long trace_line_colour(unsigned long colour)
{
    nested_event(48,(line_entry[1]&0xffff)|line_entry[2]<<16,(line_entry[3]&0xffff)|line_entry[4]<<16,0);
    return pack_colour(colour);
}
/* F1874 -> F1876: record the surface's word, its resolved origin (view) and the
   three axis vectors, then return without running F1876 (surface mode only; the
   ordinary station run never reaches F1874's call in the models compared today).
   Event 50 [word0, word6, word10]; 51 [origin x,y,z]; 52..54 each axis vector. */
extern long ref_surface(long output, long compare, long object, long instance, long entries,
                        long origin, long vectors, long word6, long word10, long word8,
                        long word0, long flag);
long trace_surface(long output, long compare, long object, long instance, long entries,
                   long origin, long vectors, long word6, long word10, long word8,
                   long word0, long flag)
{
    long *o=(long *)origin, *v=(long *)vectors;
    nested_event(50,(unsigned long)(word0&0xffff),(unsigned long)(word6&0xffff),(unsigned long)(word10&0xffff));
    nested_event(51,(unsigned long)o[0],(unsigned long)o[1],(unsigned long)o[2]);
    nested_event(52,(unsigned long)v[0],(unsigned long)v[1],(unsigned long)v[2]);
    nested_event(53,(unsigned long)v[3],(unsigned long)v[4],(unsigned long)v[5]);
    nested_event(54,(unsigned long)v[6],(unsigned long)v[7],(unsigned long)v[8]);
    {
        /* Also record what F1876 queues: the words the order cursor gains during
           the call, in order. Function pointers in them are build addresses, so
           this is porting documentation, not a numeric oracle. */
        unsigned long *before=ref_order_cursor, i, words, parts;
        long result=ref_surface(output,compare,object,instance,entries,origin,vectors,
                                word6,word10,word8,word0,flag);
        words=(unsigned long)(ref_order_cursor-before);
        nested_event(60,(unsigned long)(before-ref_order_base),words,0);
        parts=words/3+(words%3!=0);
        for(i=0;i<parts && i<100;++i)
            nested_event(61,before[3*i],3*i+1<words?before[3*i+1]:0,3*i+2<words?before[3*i+2]:0);
        return result;
    }
}
long *trace_text_vertex(void *instance, long index) { nested_event(1,(unsigned char)index,0,0); return nested_station ? ref_view_vertex(instance,(signed char)index) : nested_vertex; }
long *trace_text_font(long selector) { nested_event(2,(unsigned long)selector,0,0); return nested_descriptor; }
void trace_text_frame(void *child, void *instance, long word) { (void)child; (void)instance; nested_event(3,(unsigned long)word,0,0); }
long trace_text_push(long vertices, long normals, long extra) { nested_event(4,(unsigned long)vertices,(unsigned long)normals,(unsigned long)extra); return nested_station; }
void trace_text_string(char *output, long string, void *scratch) { (void)scratch; output[0]=0; nested_event(5,(unsigned long)string,0,0); }
void trace_text_pop(void) { nested_event(6,0,0,0); }
long *trace_submodel_vertex(void *instance, long index) { nested_event(7,(unsigned char)index,0,0); return nested_station ? ref_project_vertex(instance,(signed char)index) : nested_vertex; }
extern void ref_child_frame(void *child, void *parent, long word);
extern void ref_child_run(void *child, void *descriptor);
extern unsigned char ref_shades[24];
extern unsigned long ref_arena_index;
long *trace_submodel_model(long index)
{
    nested_event(8,(unsigned long)index,0,0);
    return child_mode ? (long *)ref_model(index) : nested_descriptor;
}
extern void ref_nested_push(unsigned long key);
extern void ref_nested_pop(void);
void trace_nested_push(unsigned long key)
{
    nested_event(12,key,0,0);
    if (queue_mode) queue_event(4,1,key);
    ref_nested_push(key);
}
void trace_nested_pop(void)
{
    nested_event(13,0,0,0);
    if (queue_mode) queue_event(4,0,0);
    ref_nested_pop();
}
extern long ref_cull(void *view, long radius);
/* Station modes cull every child regardless of the scale adjustment. */
long trace_submodel_cull(void *view, long radius)
{
    long culled;
    if (!child_mode) return nested_station ? 1 : ref_cull(view,radius);
    culled=ref_cull(view,radius);
    nested_event(19,(unsigned long)culled,(unsigned long)radius,0);
    return culled;
}
static unsigned long child_word;
void trace_submodel_frame(void *child, void *instance, long word)
{
    nested_event(9,(unsigned long)word,0,0);
    child_word=(unsigned long)word;
    if (child_mode) ref_child_frame(child,instance,word);
}
static uint64_t child_hash(const unsigned char *data,size_t size,uint64_t hash)
{
    size_t i;
    for(i=0;i<size;++i) {hash^=data[i];hash*=UINT64_C(1099511628211);}
    return hash;
}
/* Child mode: record the instance F1750/F1740 built (only fields the original
   defines), run the real F1773 and report whether F1774 ran it (+0xf0 becomes 2). */
void trace_submodel_run(void *child, void *descriptor)
{
    unsigned char *c=(unsigned char *)child;
    unsigned long *w=(unsigned long *)child;
    uint64_t hash;
    if (!child_mode) { (void)descriptor; nested_event(10,0,0,0); return; }
    hash=child_hash(c+0x14,36,UINT64_C(14695981039346656037));
    hash=child_hash(c+0x54,12,hash);
    nested_event(20,(unsigned long)child_depth+1,w[0x148/4],(unsigned long)c[0xf4]|(unsigned long)c[0xf5]<<8|(unsigned long)c[0xf6]<<16);
    nested_event(23,w[1],w[2],w[3]);
    nested_event(24,(unsigned long)hash,(unsigned long)(hash>>32),w[0x130/4]);
    /* The child's +0xf8 base handedness is stack memory the original never writes
       for a child whose own handedness is undefined, so it is reported apart and
       is not compared. */
    nested_event(26,w[0xf8/4],0,0);
    if (!(child_word&0x4000)) {
        hash=child_hash(c+0x48,12,UINT64_C(14695981039346656037));
        hash=child_hash(c+0x38,12,hash);
        nested_event(25,w[0x44/4],(unsigned long)hash,(unsigned long)(hash>>32));
    }
    w[0xf0/4]=0;
    ++child_depth;
    ref_child_run(child,descriptor);
    --child_depth;
    nested_event(21,w[0xf0/4]==2,0,0);
}

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
    if(++execution_faces>4096) {fprintf(stderr,"unbounded execution probe emissions\n");exit(2);}
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

static int jitter_vertex_vectors(int only)
{
    static const unsigned long seeds[][2]={{0x12345678UL,0x456789abUL},{0x9e3779b9UL,0x7f4a7c15UL}};
    unsigned long instance[0x160/4], globals[0x120/4];
    long vertices[256*9], coordinates[128*3];
    unsigned long m,s,i;
    fprintf(stderr,"JJFFE jitter-vertices-v1: F1472/F1478 over every model with opcode 7/8 vertex records, two F1476 seeds; object coordinates and final generator state, no commands or projection.\n");
    for (m=0;m<466;++m) {
        unsigned long *model=ref_model((int)m);
        const unsigned char *records;
        int jittered=0;
        if(only>=0 && (int)m!=only) continue;
        if(!model || model[2]==0 || model[2]>128) continue;
        records=(const unsigned char *)model[1];
        for(i=0;i<model[2]/2;++i) if(records[i*6]==7 || records[i*6]==8) jittered=1;
        if(!jittered) continue;
        for (s=0;s<2;++s) {
            memset(instance,0,sizeof(instance));memset(globals,0,sizeof(globals));memset(vertices,0,sizeof(vertices));
            ref_vertex_buffer=vertices+128*9;
            instance[0x14/4]=2147483647;instance[0x24/4]=2147483647;instance[0x34/4]=2147483647;
            instance[0xf0/4]=1;
            instance[0x148/4]=model[5];
            instance[0x14c/4]=(unsigned long)globals;
            instance[0x150/4]=model[1];
            ref_jitter_a=seeds[s][0];ref_jitter_b=seeds[s][1];
            /* Like every command caller, evaluate only on a cache miss: F1472 itself
               re-evaluates (and re-draws a jittered pair) on every call. */
            for (i=0;i<model[2];++i) {
                long *vertex=ref_vertex_buffer+i*9;
                if(!vertex[5]) vertex=ref_vertex(instance,(int)i);
                memcpy(coordinates+i*3,vertex+6,12);
            }
            fflush(stdout);
            if(getenv("FFE_JITTER_DUMP")) for(i=0;i<model[2];++i) fprintf(stderr,"v%lu %ld %ld %ld\n",i,coordinates[i*3],coordinates[i*3+1],coordinates[i*3+2]);
            printf("{\"model\":%lu,\"seed\":%lu,\"vertices\":%lu,\"coordinates\":\"%016llx\",\"state\":[%lu,%lu]}\n",
                m,s,model[2],(unsigned long long)fingerprint((unsigned char *)coordinates,model[2]*12),ref_jitter_a,ref_jitter_b);
        }
    }
    return 0;
}

static int vertex_program_vector(int index)
{
    static unsigned long instance[0x160/4], globals[0x120/4];
    static long vertices[256*9], coordinates[128*3];
    static unsigned char entries[128*16];
    unsigned long *model,i;
    fprintf(stderr,"JJFFE vertex-program-v1: every vertex of one model slot through F1472/F1478 in index order, cache-checked like command callers; identity frame, descriptor scale, start-up F1476 seed. No commands or projection.\n");
    if(index<0 || index>465) fail("model index outside the table");
    model=ref_model(index);
    if(!model || model[2]==0 || model[2]>128) { printf("{\"model\":%d,\"null\":1}\n",index); return 0; }
    memset(instance,0,sizeof(instance));memset(globals,0,sizeof(globals));memset(vertices,0,sizeof(vertices));
    ref_vertex_buffer=vertices+128*9;
    instance[0x14/4]=2147483647;instance[0x24/4]=2147483647;instance[0x34/4]=2147483647;
    instance[0xf0/4]=1;
    instance[0x148/4]=model[5];
    instance[0x14c/4]=(unsigned long)globals;
    instance[0x150/4]=model[1];
    ref_jitter_a=0x12345678UL;ref_jitter_b=0x456789abUL;
    for (i=0;i<model[2];++i) {
        long *vertex=ref_vertex_buffer+i*9;
        if(!vertex[5]) vertex=ref_vertex(instance,(int)i);
        memcpy(coordinates+i*3,vertex+6,12);
        /* The entry's screen words (+0,+2) and view dwords (+4,+8,+0xc). */
        memcpy(entries+i*16,vertex,16);
    }
    if(getenv("FFE_VERTEX_DUMP")) for(i=0;i<model[2];++i) fprintf(stderr,"v%lu %ld %ld %ld\n",i,coordinates[i*3],coordinates[i*3+1],coordinates[i*3+2]);
    printf("{\"model\":%d,\"vertices\":%lu,\"records\":\"%016llx\",\"coordinates\":\"%016llx\",\"entries\":\"%016llx\",\"state\":[%lu,%lu]}\n",
        index,model[2],(unsigned long long)fingerprint((const unsigned char *)model[1],model[2]/2*6),
        (unsigned long long)fingerprint((unsigned char *)coordinates,model[2]*12),
        (unsigned long long)fingerprint(entries,model[2]*16),ref_jitter_a,ref_jitter_b);
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

/* F1824's 32-bit mix and F1825's MSB of the largest |coordinate|. */
extern long ref_surface_mix(long x);
extern long ref_surface_vector_msb(long *point);
static int surface_helper_vectors(void)
{
#ifndef FFE_COMMAND_PROBE
    fail("--surface-helpers requires the separate instrumented jjffe-commands.exe");
    return 2;
#else
    static const unsigned long mixes[] = {0,1,2,0xffff,0x10000,0x7fffffff,0x80000000UL,0xdeadbeefUL};
    static const long points[][3] = {
        {0,0,0}, {1,0,0}, {-1,0,0}, {0x100,0,-0x100}, {0x10000,0,0},
        {-0x10000,-1,1}, {0x7fffffff,0,0}, {-2147483647L-1,1,1}, {0xffff,0x10000,0x100},
    };
    unsigned long i;
    fprintf(stderr,"JJFFE surface-helpers-v1: F1824 on 8 words and F1825 on 9 points (MSB of the largest absolute coordinate).\n");
    for(i=0;i<sizeof(mixes)/sizeof(mixes[0]);++i)
        printf("{\"mix\":%lu,\"result\":%lu}\n",mixes[i],ref_surface_mix((long)mixes[i]));
    for(i=0;i<sizeof(points)/sizeof(points[0]);++i) {
        long p[3]={points[i][0],points[i][1],points[i][2]};
        printf("{\"point\":[%ld,%ld,%ld],\"msb\":%ld}\n",p[0],p[1],p[2],ref_surface_vector_msb(p));
    }
    return 0;
#endif
}

/* F1875: the sign of a.x*b.y - a.y*b.x as the original returns it (0 or -1). */
extern long ref_surface_sign(long *a, long *b);
static int surface_sign_vectors(void)
{
#ifndef FFE_COMMAND_PROBE
    fail("--surface-sign requires the separate instrumented jjffe-commands.exe");
    return 2;
#else
    static const long pairs[][4] = {
        {1,0,0,1}, {0,1,1,0}, {3,4,-4,3}, {0,0,1,1}, {-1,0,0,-1},
        {0x40000000L,1,1,0x40000000L}, {-0x40000000L,0x7fffffffL,0x7fffffffL,-0x40000000L},
        {-2147483647L-1,1,1,-2147483647L-1}, {2147483647L,-2147483647L,-2147483647L,2147483647L},
        {0x10000,0x10000,-0x10000,0x10000}, {-0x10000,0x10000,0x10000,0x10000},
        {123456789,-987654321,987654321,123456789},
    };
    unsigned long i;
    fprintf(stderr,"JJFFE surface-sign-v1: F1875 on 12 synthetic vector pairs (0 or -1, exact 64-bit difference).\n");
    for(i=0;i<sizeof(pairs)/sizeof(pairs[0]);++i) {
        long a[2]={pairs[i][0],pairs[i][1]},b[2]={pairs[i][2],pairs[i][3]};
        printf("{\"a\":[%ld,%ld],\"b\":[%ld,%ld],\"sign\":%ld}\n",a[0],a[1],b[0],b[1],ref_surface_sign(a,b));
    }
    return 0;
#endif
}

/* Optional instance z for the surface runs: these patches are planet-scale and
   a z of 2000 clips them all. */
static long station_depth;
static int station_model_vector(int index)
{
#ifndef FFE_COMMAND_PROBE
    fail("--station-model requires the separate instrumented jjffe-commands.exe");
    return 2;
#else
    static unsigned char normals[32768*8];
    static unsigned long model_queue[65536];
    static unsigned long instance[0x160/4],globals[0x120/4],slots[128*9];
    static const long identity[9]={2147483647,0,0,0,2147483647,0,0,0,2147483647};
    unsigned long m,i;
    unsigned char *colour;
    fprintf(stderr,"JJFFE station-model-v1: one model table slot through original F1772 as in station-v1, with the descriptor scale at +0x148 and position z 2000. Not pixels or render order.\n");
    for(m=0;m<1;++m) {
        unsigned long *model;
        if(index<0 || index>465) fail("model index outside the table");
        model=ref_model(index);
        if(!model) { printf("{\"model\":%d,\"null\":1}\n",index); return 0; }
        memset(instance,0,sizeof(instance));memset(globals,0,sizeof(globals));
        memset(slots,0,sizeof(slots));memset(normals,0,sizeof(normals));
        memset(nested_descriptor,0,sizeof(nested_descriptor));
        memcpy((char *)instance+0x14,identity,36);
        memcpy((char *)instance+0x9c,identity,36);
        instance[0xc/4]=station_depth ? station_depth : 2000;
        instance[0x148/4]=model[5];
        instance[0xf0/4]=2;
        instance[0x14c/4]=(unsigned long)globals;
        instance[0x150/4]=model[1];
        /* Normal records, as F1774 sets them for any model; polygon circles (F1736)
           and beams (F1761) read them. */
        instance[0x154/4]=model[3];
        ref_vertex_buffer=(long *)slots;
        ref_normal_buffer=normals;
        ref_detail_primary=ref_detail_secondary=0;
        nested_descriptor[0x1c/4]=-0x40000000L;
        memset(model_queue,0,sizeof(model_queue));
        ref_order_base=model_queue;ref_order_active=0;ref_order_state=0;
        ref_order_init(model_queue);
        /* The game seeds F1476 at start-up; this adapter does not run that code. */
        ref_jitter_a=0x12345678UL;ref_jitter_b=0x456789abUL;
        nested_station=1;nested_count=0;point_line_count=0;
        execution_probe=1;execution_faces=0;execution_hash=UINT64_C(14695981039346656037);
        ref_command_alive=1;
        ref_execute(instance,(void *)model[0]);
        execution_probe=0;nested_station=0;
        colour=(unsigned char *)instance+0xf4;
        printf("{\"model\":%d,\"faces\":%lu,\"emissions\":\"%016llx\",\"points\":[",index,execution_faces,(unsigned long long)execution_hash);
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

static int child_model_vector(int index, long depth)
{
#ifndef FFE_COMMAND_PROBE
    fail("--child-model requires the separate instrumented jjffe-commands.exe");
    return 2;
#else
    static unsigned char normals[32768*8];
    static unsigned long model_queue[65536];
    static unsigned long instance[0x160/4],globals[0x120/4],slots[128*9];
    static const long identity[9]={2147483647,0,0,0,2147483647,0,0,0,2147483647};
    static const long light[3]={0x3000,-0x5000,0x2000};
    unsigned long *model,i;
    unsigned char *colour;
    fprintf(stderr,"JJFFE child-model-v1: one model table slot through original F1772 as in station-model-v1, but child calls run the real F1538/F1687/F1740/F1773 (text glyphs still skipped); faces drawn by the real F1708/F1712 with every queue operation logged. Not pixels.\n");
    if(index<0 || index>465) fail("model index outside the table");
    model=ref_model(index);
    if(!model) { printf("{\"model\":%d,\"null\":1}\n",index); return 0; }
    memset(instance,0,sizeof(instance));memset(globals,0,sizeof(globals));
    memset(slots,0,sizeof(slots));memset(normals,0,sizeof(normals));
    memcpy((char *)instance+0x14,identity,36);
    memcpy((char *)instance+0x9c,identity,36);
    /* Position z 2000 by default; --child-model N Z places the model deeper (e.g.
       0x50000) so far vertices clear F1738's 0x200 depth test. */
    instance[0xc/4]=(unsigned long)depth;
    instance[0x148/4]=model[5];
    instance[0xf0/4]=2;
    instance[0x14c/4]=(unsigned long)globals;
    instance[0x150/4]=model[1];
    memcpy((char *)instance+0x134,light,12);
    /* DATA_008831 shades: a test pattern, copied to +0x84 as F1683 would. */
    for(i=0;i<24;++i) ref_shades[i]=(unsigned char)(i*29+3);
    memcpy((char *)instance+0x84,ref_shades,24);
    ref_arena_index=0;
    ref_vertex_buffer=(long *)slots;
    ref_normal_buffer=normals;
    ref_detail_primary=ref_detail_secondary=0;
    memset(model_queue,0,sizeof(model_queue));
    ref_order_base=model_queue;ref_order_active=0;ref_order_state=0;
    ref_order_init(model_queue);
    ref_jitter_a=0x12345678UL;ref_jitter_b=0x456789abUL;
    queue_mode=1;queue_count=0;child_mode=1;child_depth=0;
    nested_station=1;nested_count=0;point_line_count=0;
    ref_command_alive=1;
    ref_execute(instance,(void *)model[0]);
    queue_mode=0;child_mode=0;nested_station=0;
    if(ref_vertex_buffer!=(long *)slots || ref_arena_index!=0) fail("child arena not restored");
    colour=(unsigned char *)instance+0xf4;
    printf("{\"model\":%d,\"points\":[",index);
    for(i=0;i<point_line_count;++i) {
        if(point_line_kind[i]==1) printf("%s[1,%lu,%ld,%lu,%lu]",i?",":"",point_line_a[i]&0xffff,(long)(short)point_line_b[i],point_line_c[i]&0xff,point_line_d[i]);
        else printf("%s[2,%lu,%lu,%lu,0]",i?",":"",point_line_a[i]&0xffff,point_line_b[i]&0xff,point_line_c[i]&0xff);
    }
    printf("],\"nested\":[");
    for(i=0;i<nested_count;++i)
        printf("%s[%lu,%lu,%lu,%lu]",i?",":"",nested_kind[i],nested_value[i][0],nested_value[i][1],nested_value[i][2]);
    printf("],\"queue\":[");
    for(i=0;i<queue_count;++i) printf("%s[%lu,%lu,%lu]",i?",":"",queue_log[i][0],queue_log[i][1],queue_log[i][2]);
    printf("],\"locals\":\"%016llx\",\"basis\":[",(unsigned long long)fingerprint((unsigned char *)instance+0x54,32));
    for(i=0;i<9;++i) printf("%s%ld",i?",":"",(long)instance[0x9c/4+i]);
    printf("],\"colour\":[%u,%u,%u],\"handedness\":%lu,\"alive\":%lu}\n",colour[0],colour[1],colour[2],instance[0xfc/4],ref_command_alive);
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

/* Textured faces. texture-raster drives the original F1644/F1647 rasterisers
   (F1643 routine choice, F1642 edge set-up, F1646/F1649 walks, F1650-F1653 spans)
   with synthetic points/descriptor dwords over the linked JJFFE bitmaps. */
extern void ref_tex_triangle(const short *p1,const short *p2,const short *p3,unsigned long tex);
extern void ref_tex_quad(const short *p1,const short *p2,const short *p3,const short *p4,unsigned long tex);
extern unsigned char ref_tex_slots[256];
extern unsigned char ref_tex_draw_triangle[], ref_tex_draw_quad[];
extern unsigned char ref_flat_draw_triangle[], ref_flat_draw_quad[];
static unsigned long texture_rng;
static long texture_random(long low,long high)
{
    texture_rng=(texture_rng*1103515245UL+12345UL)&0xffffffffUL;
    return low+(long)((texture_rng>>8)%(unsigned long)(high-low+1));
}
static void texture_box(short points[4][2],long cx,long cy,long rx,long ry)
{
    static const int sx[4]={-1,1,1,-1},sy[4]={-1,-1,1,1};
    short box[4][2];
    long i,start=texture_random(0,3),step=texture_random(0,1)?1:3;
    for(i=0;i<4;++i) {
        box[i][0]=(short)(cx+sx[i]*rx+texture_random(-rx/3,rx/3));
        box[i][1]=(short)(cy+sy[i]*ry+texture_random(-ry/3,ry/3));
    }
    for(i=0;i<4;++i) {points[i][0]=box[(start+i*step)&3][0];points[i][1]=box[(start+i*step)&3][1];}
}
static int texture_raster_vectors(void)
{
    static const unsigned long bitmaps[]={50,53,74,338,94,81,99,113,51,230};
    static const short xs[]={-10,0,5,160,319,320,330},ys[]={-5,0,1,79,157,158,170};
    static unsigned char buffer[320*200];
    short points[4][2];
    unsigned long n,i,count,tex;
    long minx,miny,maxx,maxy,kind,form;
    fprintf(stderr,"JJFFE texture-raster-v1: original F1644/F1647 (F1643, F1642, F1646/F1649, F1650-F1653) on synthetic screen points and descriptor dwords over the linked JJFFE bitmaps; DATA_009252[i]=(7i+1)&0x7f; 320x200 buffer filled with 0xff and hashed. Not queueing, colours or palette.\n");
    for(i=0;i<256;++i) ref_tex_slots[i]=(unsigned char)((i*7+1)&0x7f);
    ref_framebuffer=buffer;
    texture_rng=0x2545f491UL;
    for(n=0;n<4096;++n) {
        kind=(n&1)?4:3;
        form=(n>>1)&3;
        tex=bitmaps[(n>>3)%10]|((unsigned long)texture_random(0,7)<<12);
        if(form==0) texture_box(points,texture_random(20,300),texture_random(20,138),texture_random(1,60),texture_random(1,60));
        else if(form==1) texture_box(points,texture_random(-200,520),texture_random(-150,300),texture_random(20,400),texture_random(20,400));
        else if(form==2) for(i=0;i<4;++i) {points[i][0]=(short)texture_random(-300,620);points[i][1]=(short)texture_random(-200,360);}
        else for(i=0;i<4;++i) {points[i][0]=xs[texture_random(0,6)];points[i][1]=ys[texture_random(0,6)];}
        memset(buffer,0xff,sizeof(buffer));
        if(kind==3) ref_tex_triangle(points[0],points[1],points[2],tex);
        else ref_tex_quad(points[0],points[1],points[2],points[3],tex);
        count=0;minx=miny=100000;maxx=maxy=-1;
        for(i=0;i<sizeof(buffer);++i) if(buffer[i]!=0xff) {
            long x=(long)(i%320),y=(long)(i/320);
            ++count;
            if(x<minx) minx=x; if(x>maxx) maxx=x; if(y<miny) miny=y; if(y>maxy) maxy=y;
        }
        printf("{\"case\":%lu,\"kind\":%ld,\"tex\":%lu,\"points\":[",n,kind,tex);
        for(i=0;i<(unsigned long)kind;++i) printf("%s[%d,%d]",i?",":"",points[i][0],points[i][1]);
        printf("],\"written\":%lu,\"bounds\":[%ld,%ld,%ld,%ld],\"pixels_fnv1a64\":\"%016llx\"}\n",
               count,count?minx:0,count?miny:0,count?maxx:0,count?maxy:0,
               (unsigned long long)fingerprint(buffer,sizeof(buffer)));
    }
    if (fflush(stdout)) fail("cannot flush texture raster vectors");
    return 0;
}

/* Lines. line-raster drives the original F1616 DrawLine (with F1601 DrawHLine)
   on synthetic on-screen end points, as F1704 queues them (x 0..319, y 0..157),
   and hashes the whole 320x200 buffer per case. */
extern void ref_draw_line(const short *p1,const short *p2,unsigned long colour);
static int line_raster_vectors(void)
{
    static const short xs[]={0,1,2,159,160,318,319},ys[]={0,1,2,78,79,156,157};
    static unsigned char buffer[320*200];
    short points[2][2];
    unsigned long n,i,count,colour;
    long minx,miny,maxx,maxy,form,length;
    fprintf(stderr,"JJFFE line-raster-v1: original F1616/F1601 on synthetic on-screen end points (x 0..319, y 0..157); 320x200 buffer filled with 0xff and hashed. Not clipping, queueing or colours.\n");
    ref_framebuffer=buffer;
    texture_rng=0x6c078965UL;
    for(n=0;n<4096;++n) {
        form=n&7;
        colour=n%254;
        points[0][0]=(short)texture_random(0,319);points[0][1]=(short)texture_random(0,157);
        if(form==0||form==1) {points[1][0]=(short)texture_random(0,319);points[1][1]=(short)texture_random(0,157);}
        else if(form==2) {points[1][0]=(short)texture_random(0,319);points[1][1]=points[0][1];}
        else if(form==3) {points[1][0]=points[0][0];points[1][1]=(short)texture_random(0,157);}
        else if(form==4||form==5) {
            length=texture_random(0,6);
            points[1][0]=(short)(points[0][0]+texture_random(-length,length));
            points[1][1]=(short)(points[0][1]+texture_random(-length,length));
            if(points[1][0]<0) points[1][0]=0; if(points[1][0]>319) points[1][0]=319;
            if(points[1][1]<0) points[1][1]=0; if(points[1][1]>157) points[1][1]=157;
        } else if(form==6) {
            /* Near the 45-degree and 1:1 slope boundaries. */
            length=texture_random(1,150);
            points[0][0]=(short)texture_random(0,160);points[0][1]=(short)texture_random(0,7);
            points[1][0]=(short)(points[0][0]+(texture_random(0,1)?length:-length/2)+texture_random(-1,1));
            points[1][1]=(short)(points[0][1]+length);
            if(points[1][0]<0) points[1][0]=0; if(points[1][0]>319) points[1][0]=319;
            if(points[1][1]>157) points[1][1]=157;
        } else {
            for(i=0;i<2;++i) {points[i][0]=xs[texture_random(0,6)];points[i][1]=ys[texture_random(0,6)];}
        }
        memset(buffer,0xff,sizeof(buffer));
        ref_draw_line(points[0],points[1],colour);
        count=0;minx=miny=100000;maxx=maxy=-1;
        for(i=0;i<sizeof(buffer);++i) if(buffer[i]!=0xff) {
            long x=(long)(i%320),y=(long)(i/320);
            ++count;
            if(x<minx) minx=x; if(x>maxx) maxx=x; if(y<miny) miny=y; if(y>maxy) maxy=y;
        }
        printf("{\"case\":%lu,\"colour\":%lu,\"points\":[[%d,%d],[%d,%d]],\"written\":%lu,\"bounds\":[%ld,%ld,%ld,%ld],\"pixels_fnv1a64\":\"%016llx\"}\n",
               n,colour,points[0][0],points[0][1],points[1][0],points[1][1],
               count,count?minx:0,count?miny:0,count?maxx:0,count?maxy:0,
               (unsigned long long)fingerprint(buffer,sizeof(buffer)));
    }
    if (fflush(stdout)) fail("cannot flush line raster vectors");
    return 0;
}

/* Curves. curve-raster drives the original F1546 (F1549 steps, F1561 clipping,
   F1616 lines) as F1811 calls it, on synthetic control points on and off the
   view. F1561 can leave an end off the view, and F1616 then writes outside it,
   so the framebuffer pointer sits 300 rows into a 320x800 buffer, all hashed. */
extern void ref_draw_curve(const short *p1,const short *p2,const short *p3,const short *p4,unsigned long colour);
static int curve_raster_vectors(void)
{
    static unsigned char buffer[320*800];
    short points[4][2];
    unsigned long n,i,count,colour;
    long minx,miny,maxx,maxy,form,cx,cy,r;
    fprintf(stderr,"JJFFE curve-raster-v1: original F1546 (F1549, F1561, F1616/F1601) on synthetic control points; framebuffer 300 rows into a 0xff-filled 320x800 buffer, hashed; bounds are view coordinates. Not queueing or colours.\n");
    ref_framebuffer=buffer+320*300;
    texture_rng=0x1b873593UL;
    for(n=0;n<4096;++n) {
        form=n&3;
        colour=n%254;
        if(form==0) for(i=0;i<4;++i) {points[i][0]=(short)texture_random(0,319);points[i][1]=(short)texture_random(0,157);}
        else if(form==1) {
            cx=texture_random(0,319);cy=texture_random(0,157);r=texture_random(0,12);
            for(i=0;i<4;++i) {points[i][0]=(short)(cx+texture_random(-r,r));points[i][1]=(short)(cy+texture_random(-r,r));}
        } else if(form==2) for(i=0;i<4;++i) {points[i][0]=(short)texture_random(-160,480);points[i][1]=(short)texture_random(-80,240);}
        else for(i=0;i<4;++i) {points[i][0]=(short)texture_random(-40,360);points[i][1]=(short)texture_random(-30,190);}
        memset(buffer,0xff,sizeof(buffer));
        ref_draw_curve(points[0],points[1],points[2],points[3],colour);
        count=0;minx=miny=100000;maxx=maxy=-100000;
        for(i=0;i<sizeof(buffer);++i) if(buffer[i]!=0xff) {
            long x=(long)(i%320),y=(long)(i/320)-300;
            ++count;
            if(x<minx) minx=x; if(x>maxx) maxx=x; if(y<miny) miny=y; if(y>maxy) maxy=y;
        }
        printf("{\"case\":%lu,\"colour\":%lu,\"points\":[",n,colour);
        for(i=0;i<4;++i) printf("%s[%d,%d]",i?",":"",points[i][0],points[i][1]);
        printf("],\"written\":%lu,\"bounds\":[%ld,%ld,%ld,%ld],\"pixels_fnv1a64\":\"%016llx\"}\n",
               count,count?minx:0,count?miny:0,count?maxx:0,count?maxy:0,
               (unsigned long long)fingerprint(buffer,sizeof(buffer)));
    }
    if (fflush(stdout)) fail("cannot flush curve raster vectors");
    return 0;
}

/* Flat faces. fill-raster drives the original F1604 (triangles) and F1607/F1608
   (quads, with F1602/F1603/F1601) on synthetic points: quads on the view, as
   F1712 queues them, triangles also off it (F1604 clips rows and x itself).
   The framebuffer pointer sits 300 rows into a 0xff-filled 320x800 buffer. */
extern void ref_fill_triangle(const short *p1,const short *p2,const short *p3,unsigned long colour);
extern void ref_fill_quad(const short *p1,const short *p2,const short *p3,const short *p4,unsigned long colour);
static int fill_raster_vectors(void)
{
    static const short xs[]={0,1,2,159,160,318,319},ys[]={0,1,2,78,79,156,157};
    static unsigned char buffer[320*800];
    short points[4][2];
    unsigned long n,i,count,colour,kind;
    long minx,miny,maxx,maxy,form,cx,cy,r;
    fprintf(stderr,"JJFFE fill-raster-v1: original F1604 and F1607/F1608 (F1602, F1603, F1601) on synthetic points (quads on the view); framebuffer 300 rows into a 0xff-filled 320x800 buffer, hashed; bounds are view coordinates. Not queueing or colours.\n");
    ref_framebuffer=buffer+320*300;
    texture_rng=0x85ebca6bUL;
    for(n=0;n<4096;++n) {
        kind=(n&1)?4:3;
        form=(n>>1)&3;
        colour=n%254;
        if(form==0) for(i=0;i<4;++i) {points[i][0]=(short)texture_random(0,319);points[i][1]=(short)texture_random(0,157);}
        else if(form==1) {
            cx=texture_random(0,319);cy=texture_random(0,157);r=texture_random(0,10);
            for(i=0;i<4;++i) {
                points[i][0]=(short)(cx+texture_random(-r,r));points[i][1]=(short)(cy+texture_random(-r,r));
                if(points[i][0]<0) points[i][0]=0; if(points[i][0]>319) points[i][0]=319;
                if(points[i][1]<0) points[i][1]=0; if(points[i][1]>157) points[i][1]=157;
            }
        } else if(form==2 && kind==3) for(i=0;i<4;++i) {points[i][0]=(short)texture_random(-300,620);points[i][1]=(short)texture_random(-200,360);}
        else if(form==2) {
            /* A convex quad: a box with jittered corners in winding order. */
            cx=texture_random(30,290);cy=texture_random(20,138);r=texture_random(2,28);
            points[0][0]=(short)(cx-r);points[0][1]=(short)(cy-r+texture_random(-r/2,r/2));
            points[1][0]=(short)(cx+r);points[1][1]=(short)(cy-r+texture_random(-r/2,r/2));
            points[2][0]=(short)(cx+r);points[2][1]=(short)(cy+r+texture_random(-r/2,r/2));
            points[3][0]=(short)(cx-r);points[3][1]=(short)(cy+r+texture_random(-r/2,r/2));
            if(texture_random(0,1)) {short t[2]={points[1][0],points[1][1]};points[1][0]=points[3][0];points[1][1]=points[3][1];points[3][0]=t[0];points[3][1]=t[1];}
        } else for(i=0;i<4;++i) {points[i][0]=xs[texture_random(0,6)];points[i][1]=ys[texture_random(0,6)];}
        memset(buffer,0xff,sizeof(buffer));
        if(kind==3) ref_fill_triangle(points[0],points[1],points[2],colour);
        else ref_fill_quad(points[0],points[1],points[2],points[3],colour);
        count=0;minx=miny=100000;maxx=maxy=-100000;
        for(i=0;i<sizeof(buffer);++i) if(buffer[i]!=0xff) {
            long x=(long)(i%320),y=(long)(i/320)-300;
            ++count;
            if(x<minx) minx=x; if(x>maxx) maxx=x; if(y<miny) miny=y; if(y>maxy) maxy=y;
        }
        printf("{\"case\":%lu,\"kind\":%lu,\"colour\":%lu,\"points\":[",n,kind,colour);
        for(i=0;i<kind;++i) printf("%s[%d,%d]",i?",":"",points[i][0],points[i][1]);
        printf("],\"written\":%lu,\"bounds\":[%ld,%ld,%ld,%ld],\"pixels_fnv1a64\":\"%016llx\"}\n",
               count,count?minx:0,count?miny:0,count?maxx:0,count?maxy:0,
               (unsigned long long)fingerprint(buffer,sizeof(buffer)));
    }
    if (fflush(stdout)) fail("cannot flush fill raster vectors");
    return 0;
}

static unsigned long texture_face_count;
static unsigned long texture_faces[1024][20];
static void texture_face_begin(void)
{
    face_entry=0;face_colour_called=0;
}
static void texture_face_end(int kind, long material, unsigned long normal)
{
    unsigned long *row,words=0,i;
    if(!texture_queue_mode) return;
    if(texture_face_count>=1024) fail("too many texture-queue faces");
    row=texture_faces[texture_face_count++];
    memset(row,0,sizeof(texture_faces[0]));
    row[0]=(unsigned long)kind;row[1]=(unsigned short)material;row[2]=normal;
    if(face_entry && face_entry[0]==(unsigned long)ref_tex_draw_triangle) {row[3]=1;words=11;}
    else if(face_entry && face_entry[0]==(unsigned long)ref_tex_draw_quad) {row[3]=1;words=12;}
    else if(face_entry && face_entry[0]==(unsigned long)ref_flat_draw_triangle) {row[3]=2;words=4;}
    else if(face_entry && face_entry[0]==(unsigned long)ref_flat_draw_quad) {row[3]=2;words=5;}
    row[4]=face_colour_called ? ((unsigned long)face_colour[0]|((unsigned long)face_colour[1]<<8)|((unsigned long)face_colour[2]<<16)) : 0xffffffffUL;
    row[5]=words;
    for(i=0;i<words;++i) row[6+i]=face_entry[1+i];
}

/* Thruster glows (opcode 9). thruster-cases calls the original F1738 handler directly
   on a synthetic instance whose two vertex cache entries already carry the instance
   generation (so F1738 uses them as they are: no F1471), with varied scales, widths,
   depths, screen points, colours and materials; line cases call F1704 directly with
   depths on both sides of the near plane. Every queue operation and entry is logged
   through the thruster/line probes. Not pixels. */
extern unsigned char *ref_thruster(void *instance, void *operands, unsigned long code);
static unsigned long thruster_rng;
static long thruster_random(long low, long high)
{
    thruster_rng=(thruster_rng*1103515245UL+12345UL)&0xffffffffUL;
    return low+(long)(((thruster_rng>>8)|(thruster_rng<<24))%(unsigned long)(high-low+1));
}
static long thruster_depth(void)
{
    switch(thruster_random(0,5)) {
    case 0: return thruster_random(-0x100,0x200);
    case 1: return thruster_random(0x201,0x1000);
    case 2: return thruster_random(0x1000,0x40000);
    case 3: return thruster_random(0x40000,0x7fffffff);
    case 4: return thruster_random(0x201,0x8000);
    default: return thruster_random(0x400,0x20000);
    }
}
static void thruster_screen(short *a, short *b)
{
    long form=thruster_random(0,5),span;
    a[0]=(short)thruster_random(-40,360);a[1]=(short)thruster_random(-40,200);
    switch(form) {
    case 0: span=0; break;
    case 1: span=3; break;
    case 2: span=20; break;
    case 3: span=200; break;
    case 4: span=4000; break;
    default: span=32767; break;
    }
    b[0]=(short)(a[0]+thruster_random(-span,span));b[1]=(short)(a[1]+thruster_random(-span,span));
    if(form==5) {a[0]=(short)thruster_random(-32768,32767);a[1]=(short)thruster_random(-32768,32767);}
    if(thruster_random(0,40)==0) a[0]=a[1]=(short)0x8002;
    if(thruster_random(0,40)==0) b[0]=b[1]=(short)0x8002;
}
static int thruster_case_vectors(void)
{
#ifndef FFE_COMMAND_PROBE
    fail("--thruster-cases requires the separate instrumented jjffe-commands.exe");
    return 2;
#else
    static unsigned long queue_buffer[65536];
    static unsigned long instance[0x160/4],slots[4*9];
    unsigned char record[8];
    unsigned char *next;
    long *a=(long *)slots,*b=(long *)slots+9,material,width,scale,key_scale,i,n,kind;
    unsigned char *colour=(unsigned char *)instance+0xf4;
    fprintf(stderr,"JJFFE thruster-cases-v1: original F1738 on synthetic instances with both vertex entries stamped with the instance generation (8192 cases), and original F1704 on synthetic entry pairs (4096 cases); keys, F1805/F1803 entries, ramp colours and queue operations logged. Not pixels.\n");
    thruster_rng=0x6d2b79f5UL;
    for(n=0;n<12288;++n) {
        kind=n<8192 ? 0 : 1;
        memset(instance,0,sizeof(instance));memset(slots,0,sizeof(slots));
        scale=thruster_random(0,31);
        key_scale=thruster_random(-4,8);
        instance[0xf0/4]=2;
        instance[0x148/4]=(unsigned long)scale;
        instance[0x130/4]=(unsigned long)key_scale;
        if(thruster_random(0,3)) {colour[0]=(unsigned char)thruster_random(0,15);colour[1]=(unsigned char)thruster_random(0,15);colour[2]=(unsigned char)thruster_random(0,15);}
        else {colour[0]=(unsigned char)thruster_random(0,255);colour[1]=(unsigned char)thruster_random(0,255);colour[2]=(unsigned char)thruster_random(0,255);}
        material=thruster_random(0,0xffff);
        if(thruster_random(0,3)) material&=0x3fff;
        else material=(material&0xf000)|thruster_random(1,147);
        if(!(material&0xc000)) material&=0x3fff;
        switch(thruster_random(0,3)) {
        case 0: width=thruster_random(-8,8); break;
        case 1: width=thruster_random(-300,300); break;
        case 2: width=thruster_random(-4000,4000); break;
        default: width=thruster_random(-32768,32767); break;
        }
        thruster_screen((short *)a,(short *)b);
        for(i=0;i<2;++i) {
            long *e=i?b:a;
            e[1]=thruster_random(-0x40000,0x40000);e[2]=thruster_random(-0x40000,0x40000);
            e[3]=kind ? thruster_random(-0x200,0x4000) : thruster_depth();
            e[5]=2;
        }
        memset(queue_buffer,0,sizeof(queue_buffer));
        ref_order_base=queue_buffer;ref_order_active=0;ref_order_state=0;
        ref_order_init(queue_buffer);
        ref_vertex_buffer=(long *)slots;
        queue_mode=1;queue_count=0;nested_count=0;
        record[0]=9;record[1]=0;
        record[2]=(unsigned char)material;record[3]=(unsigned char)(material>>8);
        record[4]=0;record[5]=1;
        record[6]=(unsigned char)width;record[7]=(unsigned char)(width>>8);
        if(kind) {ref_line_queue(instance,a,b,(unsigned long)colour[0]|(unsigned long)colour[1]<<8|(unsigned long)colour[2]<<16);next=record+8;}
        else next=ref_thruster(instance,record+2,9);
        queue_mode=0;
        printf("{\"case\":%ld,\"kind\":%ld,\"scale\":%ld,\"key_scale\":%ld,\"colour\":[%u,%u,%u],\"material\":%ld,\"width\":%ld,\"a\":[%d,%d,%ld,%ld,%ld],\"b\":[%d,%d,%ld,%ld,%ld],\"advance\":%ld,\"nested\":[",
            n,kind,scale,key_scale,colour[0],colour[1],colour[2],material,width,
            ((short *)a)[0],((short *)a)[1],a[1],a[2],a[3],((short *)b)[0],((short *)b)[1],b[1],b[2],b[3],(long)(next-record));
        for(i=0;i<(long)nested_count;++i)
            printf("%s[%lu,%lu,%lu,%lu]",i?",":"",nested_kind[i],nested_value[i][0],nested_value[i][1],nested_value[i][2]);
        printf("],\"queue\":[");
        for(i=0;i<(long)queue_count;++i) printf("%s[%lu,%lu,%lu]",i?",":"",queue_log[i][0],queue_log[i][1],queue_log[i][2]);
        printf("]}\n");
    }
    return 0;
#endif
}

static int texture_queue_vector(int index)
{
#ifndef FFE_COMMAND_PROBE
    fail("--texture-queue requires the separate instrumented jjffe-commands.exe");
    return 2;
#else
    static unsigned char normals[32768*8];
    static unsigned long queue_buffer[65536];
    static unsigned long instance[0x160/4],globals[0x120/4],slots[128*9];
    static const long identity[9]={2147483647,0,0,0,2147483647,0,0,0,2147483647};
    static const long tilted[9]={1518500249,0,-1518500249,0,2147483647,0,1518500249,0,1518500249};
    static const long positions[][3]={{0,0,2000},{0,0,600},{300,-200,3000},{0,0,12000}};
    unsigned long v,i,j;
    unsigned long *model;
    fprintf(stderr,"JJFFE texture-queue-v1: one model slot through original F1772 with real F1708/F1712 (as station-model-v1, four views, instance colour 1,2,3, synthetic normal colours r=3i%%9 g=5i%%10 b=7i%%11, culled when i%%7==3); per face the queue entry of the textured (F1807/F1805) or on-screen flat (F1804/F1809) path and the F1691 flat colour. Points/lines/text/children not queued; no flush or pixels.\n");
    if(index<0 || index>465) fail("model index outside the table");
    model=ref_model(index);
    if(!model) { printf("{\"model\":%d,\"null\":1}\n",index); return 0; }
    for(i=0;i<32768;++i) {
        normals[8*i]=(unsigned char)(i*3%9);normals[8*i+1]=(unsigned char)(i*5%10);
        normals[8*i+2]=(unsigned char)(i*7%11);normals[8*i+3]=(unsigned char)(i%7==3);
        normals[8*i+4]=normals[8*i+5]=normals[8*i+6]=normals[8*i+7]=0;
    }
    for(v=0;v<4;++v) {
        memset(instance,0,sizeof(instance));memset(globals,0,sizeof(globals));
        memset(slots,0,sizeof(slots));
        memset(nested_descriptor,0,sizeof(nested_descriptor));
        memcpy((char *)instance+0x14,v>=2?tilted:identity,36);
        memcpy((char *)instance+0x9c,identity,36);
        memcpy((char *)instance+4,positions[v],12);
        ((unsigned char *)instance)[0xf4]=1;((unsigned char *)instance)[0xf5]=2;((unsigned char *)instance)[0xf6]=3;
        instance[0x148/4]=model[5];
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
        ref_jitter_a=0x12345678UL;ref_jitter_b=0x456789abUL;
        queue_mode=1;queue_count=0;texture_queue_mode=1;texture_face_count=0;
        nested_station=1;nested_count=0;point_line_count=0;
        ref_command_alive=1;
        ref_execute(instance,(void *)model[0]);
        queue_mode=0;texture_queue_mode=0;nested_station=0;
        printf("{\"model\":%d,\"view\":%lu,\"faces\":[",index,v);
        for(i=0;i<texture_face_count;++i) {
            unsigned long *row=texture_faces[i];
            printf("%s[%lu,%lu,%lu,%lu,%ld",i?",":"",row[0],row[1],row[2],row[3],row[4]==0xffffffffUL?-1L:(long)row[4]);
            for(j=0;j<row[5];++j) printf(",%lu",row[6+j]);
            printf("]");
        }
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

/* Polygon helpers called directly: F1467 table root, F1564 near-plane point, F1557
   circle point, F1554 circle (F1555/F1556 inside) and F1547 curve subdivision with
   a recording edge callback in place of F1560. Inputs are fixed boundary cases plus
   a seeded xorshift; nothing here depends on game state. */
extern unsigned long ref_table_root(unsigned long x);
extern void ref_near_point(void *out, const long *behind, const long *front);
extern long ref_circle_point(void *out, const long *point);
extern long ref_circle(void *buffer, const long *centre, const long *axis, long radius);
extern void ref_curve(void *buffer, const unsigned long *a, const unsigned long *b,
    const unsigned long *c, const unsigned long *d,
    void (*edge)(void *, unsigned long *, unsigned long *, long), long arg);
static unsigned long helper_state = 0x2545f491UL;
static unsigned long helper_random(void)
{
    helper_state ^= helper_state << 13; helper_state &= 0xffffffffUL;
    helper_state ^= helper_state >> 17;
    helper_state ^= helper_state << 5; helper_state &= 0xffffffffUL;
    return helper_state;
}
/* A signed value of random magnitude: shifted right by 0..31. */
static long helper_value(void) { long v = (long)helper_random(); return v >> (helper_random() % 32); }
static unsigned long curve_edges[64][2], curve_edge_count;
static void curve_edge(void *buffer, unsigned long *a, unsigned long *b, long arg)
{
    (void)buffer; (void)arg;
    if (curve_edge_count >= 64) { fprintf(stderr,"too many curve edges\n"); exit(2); }
    curve_edges[curve_edge_count][0]=*a; curve_edges[curve_edge_count][1]=*b; curve_edge_count++;
}
#define SCREEN_X(p) ((long)(short)((p)&0xffff))
#define SCREEN_Y(p) ((long)(short)((p)>>16))
static int polygon_helper_vectors(void)
{
    static const unsigned long limits[] = {0x400UL,0x4000UL,0x40000UL,0x400000UL,0x4000000UL,0x10000000UL,0x40000000UL,0x80000000UL};
    unsigned long i, j, out;
    fprintf(stderr,"JJFFE polygon-helpers-v1: F1467, F1564, F1557, F1554 and F1547 called directly on synthetic inputs. Not rasterisation.\n");
    for (i=0;i<2048+8*5+2048;++i) {
        unsigned long x;
        if (i<2048) x=i;
        else if (i<2048+40) x=limits[(i-2048)/5]+(i-2048)%5-2;
        else x=helper_random()>>(helper_random()%32);
        printf("{\"k\":\"root\",\"in\":[%lu],\"out\":[%lu]}\n",x,ref_table_root(x));
    }
    for (i=0;i<2048;++i) {
        long behind[3], front[3];
        behind[0]=helper_value(); behind[1]=helper_value(); behind[2]=i ? 0x3f-(long)(helper_random()>>(helper_random()%32)) : 0x3f;
        front[0]=helper_value(); front[1]=helper_value(); front[2]=i ? 0x40+(long)(helper_random()>>(1+helper_random()%31)) : 0x40;
        out=0; ref_near_point(&out,behind,front);
        printf("{\"k\":\"near\",\"in\":[%ld,%ld,%ld,%ld,%ld,%ld],\"out\":[%ld,%ld]}\n",
            behind[0],behind[1],behind[2],front[0],front[1],front[2],SCREEN_X(out),SCREEN_Y(out));
    }
    for (i=0;i<2048;++i) {
        long point[3]; long ok;
        point[0]=helper_value(); point[1]=helper_value(); point[2]=helper_value();
        out=0; ok=ref_circle_point(&out,point);
        printf("{\"k\":\"point\",\"in\":[%ld,%ld,%ld],\"out\":[%ld,%ld,%ld]}\n",
            point[0],point[1],point[2],ok,ok?SCREEN_X(out):0,ok?SCREEN_Y(out):0);
    }
    for (i=0;i<2048;++i) {
        long centre[3], axis[3], radius, ok;
        unsigned long buffer[0x98/4];
        centre[0]=helper_value()>>4; centre[1]=helper_value()>>4; centre[2]=helper_value()>>2;
        for (j=0;j<3;++j) axis[j]=(i%4) ? (long)(helper_random()%255)-127 : helper_value()>>(helper_random()%2 ? 16 : 1);
        radius=(long)((helper_random()&255)<<(helper_random()%16));
        memset(buffer,0,sizeof(buffer));
        ok=ref_circle(buffer,centre,axis,radius);
        printf("{\"k\":\"circle\",\"in\":[%ld,%ld,%ld,%ld,%ld,%ld,%ld],\"out\":[%ld",
            centre[0],centre[1],centre[2],axis[0],axis[1],axis[2],radius,ok);
        if (ok) for (j=0;j<8;++j) printf(",%ld,%ld",SCREEN_X(buffer[0x6c/4+j]),SCREEN_Y(buffer[0x6c/4+j]));
        printf("]}\n");
    }
    for (i=0;i<2048;++i) {
        unsigned long points[4];
        unsigned long range = i%3==0 ? 50 : i%3==1 ? 8000 : 32767;
        for (j=0;j<4;++j) {
            long x=(long)(helper_random()%(2*range+1))-(long)range, y=(long)(helper_random()%(2*range+1))-(long)range;
            points[j]=((unsigned long)x&0xffff)|((unsigned long)y<<16);
        }
        curve_edge_count=0;
        ref_curve(NULL,&points[0],&points[1],&points[2],&points[3],curve_edge,0);
        printf("{\"k\":\"curve\",\"in\":[");
        for (j=0;j<4;++j) printf("%s%ld,%ld",j?",":"",SCREEN_X(points[j]),SCREEN_Y(points[j]));
        printf("],\"out\":[");
        for (j=0;j<curve_edge_count;++j) printf("%s%ld,%ld,%ld,%ld",j?",":"",
            SCREEN_X(curve_edges[j][0]),SCREEN_Y(curve_edges[j][0]),SCREEN_X(curve_edges[j][1]),SCREEN_Y(curve_edges[j][1]));
        printf("]}\n");
    }
    if (fflush(stdout)) fail("cannot flush polygon helper vectors");
    return 0;
}

/* Animation oracle (--animation, both binaries): the original F1675 MatBuildYZT
   and F866 (orbital update: F865 door word and the rotation step) on synthetic
   object records. Object-instance oracle (--object-instance, probe binary): the
   original F1537 frame loop and F1680 draw set-up on a synthetic object list,
   and F1683 on synthetic display objects, with F1773 (model run) and F1700 (dot)
   redirected inside F1680/F1683 so the instance is captured, not executed. */
extern void ref_mat_build_yzt(long *matrix, long a, long b);
extern void ref_orbital_update(void *parameters);
extern unsigned long ref_frame_time;
static unsigned long animation_state = 0x2468ace1UL;
static unsigned long animation_random(void)
{
    animation_state ^= animation_state << 13; animation_state &= 0xffffffffUL;
    animation_state ^= animation_state >> 17;
    animation_state ^= animation_state << 5; animation_state &= 0xffffffffUL;
    return animation_state;
}

static int animation_vectors(void)
{
    static const long specials[] = {-1, 0x10000, 0x7fffffffL, (-0x7fffffffL - 1), 0x12345678L, 0x4000, 0x3fff, 0xc001};
    static const long seconds[] = {0, 0x4000, 0x8000, 0xc000, 1, 0x1234, 0xffff, -0x4000, 0x10000, 0x7fffffffL};
    static const long speeds[] = {0, 1, 2, 9, 10, 11, 12, 13, 31, 32, 33, 127, -1, -128};
    static const unsigned long frames[] = {0, 1, 2, 994, 9940, 0x1fffeUL, 0x7fffffffUL, 0xffffffffUL};
    static const signed char states[] = {-128, -3, -2, -1, 0, 1, 2, 3, 127};
    static const unsigned char flags[] = {0x10, 0x00, 0x20, 0x30, 0x11};
    static unsigned char object[0x160];
    long matrix[9], a, b;
    unsigned long i, j, k;
    fprintf(stderr, "JJFFE animation-v1: original F1675 MatBuildYZT and F866 orbital update (F865 door word, rotation step) on synthetic orbital records; zero angular velocity, so no orbital position update. Not model execution.\n");
    for (i = 0; i < 264; ++i) for (j = 0; j < 10; ++j) {
        a = i < 256 ? (long)(i * 0x101) : specials[i - 256];
        b = seconds[j];
        for (k = 0; k < 9; ++k) matrix[k] = 0x5a5a5a5aL;
        ref_mat_build_yzt(matrix, a, b);
        printf("{\"k\":\"yzt\",\"a\":%ld,\"b\":%ld,\"m\":[", a, b);
        for (k = 0; k < 9; ++k) printf("%s%ld", k ? "," : "", matrix[k]);
        printf("]}\n");
    }
    for (i = 0; i < 4096; ++i) {
        unsigned long parameters[3], previous, tics, frame, word;
        long speed, orientation;
        signed char state;
        unsigned char flag;
        unsigned long *m = (unsigned long *)object;
        memset(object, 0, sizeof(object));
        for (k = 0; k < 9; ++k) m[k] = 0x5a5a5a5aUL;
        previous = animation_random();
        switch (i % 4) {
        case 0: tics = animation_random(); break;
        case 1: tics = previous + (animation_random() & 0x3ff); break;
        case 2: tics = previous + 994; break;
        default: tics = previous + (animation_random() & 0xfffff); break;
        }
        tics &= 0xffffffffUL;
        flag = flags[animation_random() % 5];
        speed = speeds[animation_random() % 14];
        orientation = (long)(short)(animation_random() & 0xffff);
        if (animation_random() % 3 == 0) orientation = 0;
        state = states[animation_random() % 9];
        word = animation_random() % 3 == 0 ? (animation_random() % 2 ? 0xffff : 0) : (animation_random() & 0xffff);
        frame = frames[animation_random() % 8];
        *(unsigned long *)(object + 0xa8) = previous;
        *(unsigned long *)(object + 0xac) = 0x121cf7UL;
        object[0x14c] = flag;
        object[0xc8] = (unsigned char)speed;
        *(unsigned short *)(object + 0xcc) = (unsigned short)orientation;
        *(unsigned short *)(object + 0x9e) = (unsigned short)word;
        object[0xc9] = (unsigned char)state;
        ref_frame_time = frame;
        parameters[0] = tics; parameters[1] = 0x121cf8UL; parameters[2] = (unsigned long)object;
        ref_orbital_update(parameters);
        printf("{\"k\":\"orbital\",\"flags\":%u,\"speed\":%ld,\"orientation\":%ld,\"previous\":%lu,\"tics\":%lu,\"frame\":%lu,\"word\":%lu,\"state\":%d,\"out_word\":%u,\"out_state\":%d,\"out_tics\":%lu,\"out_days\":%lu,\"m\":",
               flag, speed, orientation, previous, tics, frame, word, state,
               *(unsigned short *)(object + 0x9e), (signed char)object[0xc9],
               *(unsigned long *)(object + 0xa8), *(unsigned long *)(object + 0xac));
        if (m[0] == 0x5a5a5a5aUL && m[8] == 0x5a5a5a5aUL) printf("null}\n");
        else {
            printf("[");
            for (k = 0; k < 9; ++k) printf("%s%ld", k ? "," : "", (long)m[k]);
            printf("]}\n");
        }
    }
    if (fflush(stdout)) fail("cannot flush animation vectors");
    return 0;
}

#ifdef FFE_COMMAND_PROBE
extern void ref_object_frame(void *list, void *camera);
extern void ref_display_object(void *object, void *descriptor);
extern long ref_light[3], ref_star_light[3];
extern long ref_light_timer;
#define OBJECT_SIZE 0x152
#define OBJECT_SLOTS 0x73
static unsigned char object_list[0x74 + OBJECT_SIZE * OBJECT_SLOTS];
static unsigned char object_capture[OBJECT_SLOTS + 1][0x160];
static int object_outcome[OBJECT_SLOTS + 1];
static int object_capture_mode;
static unsigned char *display_target;
/* Inside F1680 / F1683: F1773(instance, descriptor) runs the model; capture the
   instance the original built and return without running it. */
static void capture_instance(void *instance, int kind)
{
    unsigned char *c = (unsigned char *)instance;
    unsigned char *owner = *(unsigned char **)(c + 0x14c);
    long slot;
    if (!object_capture_mode) fail("instance capture outside the object-instance mode");
    if (owner == display_target) slot = 0;
    else {
        slot = (long)(owner - (object_list + 0x74)) / OBJECT_SIZE;
        if (slot < 1 || slot > OBJECT_SLOTS - 1 || owner != object_list + 0x74 + slot * OBJECT_SIZE)
            fail("captured instance names an unknown object");
    }
    if (object_outcome[slot]) fail("object drawn twice");
    memcpy(object_capture[slot], c, 0x160);
    object_outcome[slot] = kind;
}
void trace_object_run(void *instance, void *descriptor) { (void)descriptor; capture_instance(instance, 1); }
void trace_display_run(void *instance, void *descriptor) { (void)descriptor; capture_instance(instance, 1); }
/* F1700(instance, ...): the distant-dot entry. Only the call is recorded. */
void trace_object_dot(void *instance) { capture_instance(instance, 2); }

static void print_instance(const unsigned char *c, const unsigned char *owner)
{
    const long *w = (const long *)c;
    unsigned long k;
    printf("{\"position\":[%ld,%ld,%ld],\"matrix\":[", w[1], w[2], w[3]);
    for (k = 0; k < 9; ++k) printf("%s%ld", k ? "," : "", w[0x14 / 4 + k]);
    printf("],\"direction\":[%ld,%ld,%ld],\"observer_scale\":%ld,\"observer\":[%ld,%ld,%ld],\"local0\":%ld,\"shades\":%d,\"key_scale\":%ld,\"light\":[%ld,%ld,%ld],\"scale\":%ld,\"owner\":%d,\"colour\":[%u,%u,%u],\"base_handedness\":%ld}",
           w[0x38 / 4], w[0x3c / 4], w[0x40 / 4], w[0x44 / 4], w[0x48 / 4], w[0x4c / 4], w[0x50 / 4],
           w[0x54 / 4], memcmp(c + 0x84, ref_shades, 24) == 0, w[0x130 / 4],
           w[0x134 / 4], w[0x138 / 4], w[0x13c / 4], w[0x148 / 4],
           *(const unsigned char * const *)(c + 0x14c) == owner, c[0xf4], c[0xf5], c[0xf6], w[0xf8 / 4]);
}

static void random_matrix(long *m)
{
    unsigned long k;
    for (k = 0; k < 9; ++k) m[k] = animation_random() % 4 == 0 ? (k % 4 == 0 ? 0x7fffffffL : 0) : (long)animation_random();
}

/* A 64-bit coordinate of random magnitude (2^0 .. 2^50), as two dwords. */
static void random_coordinate(unsigned char *at)
{
    unsigned long bits = animation_random() % 51, low, high;
    uint64_t value = ((uint64_t)animation_random() << 32 | animation_random());
    value = bits >= 63 ? value : value >> (63 - bits);
    if (animation_random() & 1) value = (uint64_t)0 - value;
    low = (unsigned long)(value & 0xffffffffUL); high = (unsigned long)(value >> 32);
    memcpy(at, &low, 4); memcpy(at + 4, &high, 4);
}

static void print_coordinates(const unsigned char *at)
{
    unsigned long k;
    for (k = 0; k < 3; ++k) {
        unsigned long low, high;
        memcpy(&low, at + 8 * k, 4); memcpy(&high, at + 8 * k + 4, 4);
        printf("%s%lld", k ? "," : "", (long long)(int64_t)((uint64_t)high << 32 | low));
    }
}

static int object_instance_vectors(void)
{
    static unsigned char camera[0x160], display[0x160];
    static unsigned long queue[65536];
    unsigned long frame, slot, k, i;
    int models[466], model_count = 0;
    fprintf(stderr, "JJFFE object-instance-v1: original F1537 frame loop (F1534 global word 0, light) and F1680 set-up on a synthetic list of objects sharing the camera's frame of reference, and F1683 on synthetic display objects; F1773/F1700 captured, models not executed.\n");
    for (i = 0; i < 466; ++i) if (ref_model((int)i)) models[model_count++] = (int)i;
    for (i = 0; i < 24; ++i) ref_shades[i] = (unsigned char)(i * 29 + 3);
    object_capture_mode = 1;
    for (frame = 0; frame < 96; ++frame) {
        unsigned long tics = animation_random(), count = 1 + frame % 40;
        memset(object_list, 0, sizeof(object_list)); memset(camera, 0, sizeof(camera));
        memset(object_outcome, 0, sizeof(object_outcome));
        random_matrix((long *)camera);
        random_coordinate(camera + 0x3e); random_coordinate(camera + 0x46); random_coordinate(camera + 0x4e);
        for (k = 0; k < 3; ++k) ref_star_light[k] = (long)(short)(animation_random() & 0xffff);
        ref_light_timer = 0x7fffffffL;
        *(unsigned long *)ref_state = tics;
        for (slot = 1; slot <= count; ++slot) {
            unsigned char *o = object_list + 0x74 + slot * OBJECT_SIZE;
            object_list[slot] = 8;
            random_matrix((long *)o);
            if (frame % 3 == 0) {
                /* Near the camera: magnitudes the far/dot thresholds do not reject. */
                memcpy(o + 0x3e, camera + 0x3e, 24);
                for (k = 0; k < 3; ++k) {
                    long delta = (long)(animation_random() % 0x200000) - 0x100000;
                    unsigned long low, high; int64_t value;
                    memcpy(&low, o + 0x3e + 8 * k, 4); memcpy(&high, o + 0x42 + 8 * k, 4);
                    value = (int64_t)((uint64_t)high << 32 | low) + delta;
                    if (k == 2 && slot % 2) value += 0x40000;
                    low = (unsigned long)((uint64_t)value & 0xffffffffUL); high = (unsigned long)((uint64_t)value >> 32);
                    memcpy(o + 0x3e + 8 * k, &low, 4); memcpy(o + 0x42 + 8 * k, &high, 4);
                }
            } else {
                random_coordinate(o + 0x3e); random_coordinate(o + 0x46); random_coordinate(o + 0x4e);
            }
            *(long *)(o + 0x7e) = (long)(animation_random() % 12) - 3;
            *(long *)(o + 0x82) = models[animation_random() % model_count];
            o[0x86] = (unsigned char)slot;
            o[0x8b] = animation_random() % 8 == 0 ? (unsigned char)(1 + animation_random() % 3) : 0;
            for (k = 0; k < 64; ++k) *(unsigned short *)(o + 0x9c + 2 * k) = (unsigned short)animation_random();
        }
        memset(queue, 0, sizeof(queue));
        ref_order_base = queue; ref_order_active = 0; ref_order_state = 0;
        ref_order_init(queue);
        /* Inputs are printed from a copy taken before the frame runs. */
        {
            static unsigned char before[sizeof(object_list)];
            memcpy(before, object_list, sizeof(object_list));
            ref_object_frame(object_list, camera);
            printf("{\"k\":\"frame\",\"tics\":%lu,\"camera\":{\"matrix\":[", tics);
            for (k = 0; k < 9; ++k) printf("%s%ld", k ? "," : "", ((long *)camera)[k]);
            printf("],\"position\":["); print_coordinates(camera + 0x3e);
            printf("]},\"star\":[%ld,%ld,%ld],\"light\":[%ld,%ld,%ld],\"objects\":[",
                   ref_star_light[0], ref_star_light[1], ref_star_light[2], ref_light[0], ref_light[1], ref_light[2]);
            for (slot = 1; slot <= count; ++slot) {
                const unsigned char *o = before + 0x74 + slot * OBJECT_SIZE;
                const unsigned char *after = object_list + 0x74 + slot * OBJECT_SIZE;
                unsigned long *d = ref_model(*(const long *)(o + 0x82));
                printf("%s{\"slot\":%lu,\"model\":%ld,\"descriptor\":[%ld,%ld,%ld,%ld,%ld],\"matrix\":[",
                       slot > 1 ? "," : "", slot, *(const long *)(o + 0x82),
                       (long)d[5], (long)d[6], (long)d[7], (long)d[8], (long)d[10]);
                for (k = 0; k < 9; ++k) printf("%s%ld", k ? "," : "", ((const long *)o)[k]);
                printf("],\"position\":["); print_coordinates(o + 0x3e);
                printf("],\"scale\":%ld,\"sleep\":%u,\"word0\":%u,\"words_kept\":%d,\"out_sleep\":%u,\"out_word0\":%u,\"distance\":%u,\"outside\":%u,\"screen\":[%u,%u],\"outcome\":%d,\"instance\":",
                       *(const long *)(o + 0x7e), o[0x8b], *(const unsigned short *)(o + 0x9c),
                       memcmp(o + 0x9e, after + 0x9e, 126) == 0, after[0x8b], *(const unsigned short *)(after + 0x9c),
                       after[0x88], after[0x8a], *(const unsigned short *)(after + 0x98), *(const unsigned short *)(after + 0x9a),
                       object_outcome[slot]);
                if (object_outcome[slot] == 1) print_instance(object_capture[slot], after);
                else printf("null");
                printf("}");
            }
            printf("]}\n");
        }
    }
    for (i = 0; i < 4096; ++i) {
        int model = models[i % model_count];
        unsigned long *d = ref_model(model);
        long *w = (long *)display;
        memset(display, 0, sizeof(display));
        memset(object_outcome, 0, sizeof(object_outcome));
        random_matrix(w);
        for (k = 0; k < 3; ++k) {
            long v = (long)animation_random() >> (animation_random() % 32);
            if (k == 2 && i % 2) v = (long)(animation_random() % 0x100000);
            *(long *)(display + 0x26 + 8 * k) = v;
        }
        for (k = 0; k < 3; ++k) ref_light[k] = (long)(short)(animation_random() & 0xffff);
        display_target = display;
        memset(queue, 0, sizeof(queue));
        ref_order_base = queue; ref_order_active = 0; ref_order_state = 0;
        ref_order_init(queue);
        ref_display_object(display, d);
        printf("{\"k\":\"display\",\"model\":%d,\"descriptor\":[%ld,%ld,%ld],\"matrix\":[", model, (long)d[5], (long)d[6], (long)d[7]);
        for (k = 0; k < 9; ++k) printf("%s%ld", k ? "," : "", w[k]);
        printf("],\"position\":[%ld,%ld,%ld],\"in_light\":[%ld,%ld,%ld],\"outcome\":%d,\"instance\":",
               *(long *)(display + 0x26), *(long *)(display + 0x2e), *(long *)(display + 0x36),
               ref_light[0], ref_light[1], ref_light[2], object_outcome[0]);
        if (object_outcome[0] == 1) print_instance(object_capture[0], display);
        else printf("null");
        printf("}\n");
    }
    object_capture_mode = 0;
    display_target = NULL;
    if (fflush(stdout)) fail("cannot flush object-instance vectors");
    return 0;
}
#else
void trace_object_run(void *instance, void *descriptor) { (void)instance; (void)descriptor; }
void trace_display_run(void *instance, void *descriptor) { (void)instance; (void)descriptor; }
void trace_object_dot(void *instance) { (void)instance; }
static int object_instance_vectors(void)
{
    fail("--object-instance requires the separate instrumented jjffe-commands binary");
    return 2;
}
#endif

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
    if (argc == 2 && !strcmp(argv[1], "--surface-sign")) return surface_sign_vectors();
    if (argc == 2 && !strcmp(argv[1], "--surface-helpers")) return surface_helper_vectors();
    if (argc == 4 && !strcmp(argv[1], "--station-model")) {
        station_depth=strtol(argv[3],0,0);
        return station_model_vector(atoi(argv[2]));
    }
    if (argc == 3 && !strcmp(argv[1], "--station-model")) return station_model_vector(atoi(argv[2]));
    if (argc == 2 && !strcmp(argv[1], "--station-queue")) return station_queue_vectors();
    if (argc == 2 && !strcmp(argv[1], "--polygon-helpers")) return polygon_helper_vectors();
    if (argc == 2 && !strcmp(argv[1], "--texture-raster")) return texture_raster_vectors();
    if (argc == 2 && !strcmp(argv[1], "--line-raster")) return line_raster_vectors();
    if (argc == 2 && !strcmp(argv[1], "--curve-raster")) return curve_raster_vectors();
    if (argc == 2 && !strcmp(argv[1], "--fill-raster")) return fill_raster_vectors();
    if (argc == 3 && !strcmp(argv[1], "--texture-queue")) return texture_queue_vector(atoi(argv[2]));
    if (argc == 3 && !strcmp(argv[1], "--child-model")) return child_model_vector(atoi(argv[2]),2000);
    if (argc == 4 && !strcmp(argv[1], "--child-model")) return child_model_vector(atoi(argv[2]),strtol(argv[3],0,0));
    if (argc == 2 && !strcmp(argv[1], "--thruster-cases")) return thruster_case_vectors();
    if (argc == 2 && !strcmp(argv[1], "--base-transform")) return base_transform_vectors();
    if (argc == 2 && !strcmp(argv[1], "--animation")) return animation_vectors();
    if (argc == 2 && !strcmp(argv[1], "--object-instance")) return object_instance_vectors();
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
    if (argc == 2 && !strcmp(argv[1], "--jitter-vertices")) return jitter_vertex_vectors(-1);
    if (argc == 3 && !strcmp(argv[1], "--jitter-vertices")) return jitter_vertex_vectors(atoi(argv[2]));
    if (argc == 3 && !strcmp(argv[1], "--vertex-program")) return vertex_program_vector(atoi(argv[2]));
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
