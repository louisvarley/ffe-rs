#include <stdint.h>
#include <stdio.h>
#include <math.h>
#include <setjmp.h>
#include <signal.h>
#include <sys/time.h>
#include <sys/personality.h>
#include <sys/wait.h>
#include <unistd.h>
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
/* Map-star mode: the real F1699 resolves its vertex (cache check, then F1471) before it draws, and
   F1769 reads the depth of vertex slot 0 afterwards; the probe must leave the same slot behind. */
static int point_projects;
extern long *ref_project_vertex(void *instance,int index);
void trace_point_args(void *instance, long material, long angle, long vertex, long flag)
{
    if (point_projects) {
        long *slots=ref_vertex_buffer;
        int k=(signed char)vertex;
        if (slots[k*9+5]!=((long *)instance)[0xf0/4]) ref_project_vertex(instance,k);
    }
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
static unsigned long *label_entries[512];
static unsigned long label_entry_count;
unsigned long *trace_label_alloc(long words, unsigned long key, long force)
{
    unsigned long *entry;
    if (queue_mode) queue_event(6,0,key);
    entry=ref_order_allocate(words,key,force);
    if (entry && label_entry_count<512) label_entries[label_entry_count++]=entry;
    return entry;
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
/* --game-frame <frames> <key> [<key>...]: the real game from the new game on, keys sent by scan code every 40 frames after frame 200, the 320x200 frame and palette written to game-frame.bin at the end. */
static int game_frame_mode;
static unsigned long game_keys[16], game_key_count, game_key_next, game_key_frame = 200;

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
    if ((state_mode || game_frame_mode) && !sent_start && frames >= 160) {
        sent_start = 1;
        return 0x02; /* Intro choice 1, after the 100-tick splash lockout. */
    }
    if (game_frame_mode && game_key_next < game_key_count && frames >= game_key_frame) {
        game_key_frame = frames + 40;
        return (long)game_keys[game_key_next++];
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
    if (game_frame_mode) {
        /* a frame hash only: the trace stays small */
        printf("{\"frame\":%lu,\"pixels_fnv1a64\":\"%016llx\"}\n", frames, (unsigned long long)fingerprint(pixels, sizeof(pixels)));
    } else if (state_mode) {
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
    if (game_frame_mode && frames + 1 >= limit) {
        FILE *out = fopen("game-frame.bin", "wb");
        if (!out) fail("cannot write game-frame.bin");
        fwrite(pixels, 1, sizeof pixels, out);
        fwrite(palette, 1, sizeof palette, out);
        fclose(out);
    }
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

/* Polygon spans. span-raster drives the original F1468 (a fresh span table),
   F1560 for every edge of a synthetic closed polygon (on and off the view,
   self-crossing, up to 12 points) and F1613 (with F1600/F1601), as F1794,
   F1795/F1796 and F1799 do. The framebuffer pointer sits 300 rows into a
   0xff-filled 320x800 buffer. */
extern void *ref_span_reset(void);
extern void ref_span_edge(void *spans,const short *p1,const short *p2,unsigned long flag);
extern void ref_span_draw(void *spans,unsigned long colour);
static int span_raster_vectors(void)
{
    static unsigned char buffer[320*800];
    short points[12][2];
    unsigned long n,i,count,colour,sides;
    long minx,miny,maxx,maxy,form,cx,cy,r;
    void *spans;
    fprintf(stderr,"JJFFE span-raster-v1: original F1468, F1560 (F1563, F1559, F1558) per edge of synthetic closed polygons and F1613 (F1600/F1601); framebuffer 300 rows into a 0xff-filled 320x800 buffer, hashed; bounds are view coordinates. Not queueing, curves or colours.\n");
    ref_framebuffer=buffer+320*300;
    texture_rng=0xc2b2ae35UL;
    for(n=0;n<4096;++n) {
        form=n&3;
        colour=n%254;
        sides=(unsigned long)texture_random(3,(n&8)?12:6);
        if(form==0) for(i=0;i<sides;++i) {points[i][0]=(short)texture_random(0,319);points[i][1]=(short)texture_random(0,157);}
        else if(form==1) {
            cx=texture_random(0,319);cy=texture_random(0,157);r=texture_random(1,40);
            for(i=0;i<sides;++i) {points[i][0]=(short)(cx+texture_random(-r,r));points[i][1]=(short)(cy+texture_random(-r,r));}
        } else if(form==2) for(i=0;i<sides;++i) {points[i][0]=(short)texture_random(-400,720);points[i][1]=(short)texture_random(-300,460);}
        else for(i=0;i<sides;++i) {points[i][0]=(short)texture_random(-60,380);points[i][1]=(short)texture_random(-40,200);}
        memset(buffer,0xff,sizeof(buffer));
        spans=ref_span_reset();
        for(i=0;i<sides;++i) ref_span_edge(spans,points[i],points[(i+1)%sides],0);
        ref_span_draw(spans,colour);
        count=0;minx=miny=100000;maxx=maxy=-100000;
        for(i=0;i<sizeof(buffer);++i) if(buffer[i]!=0xff) {
            long x=(long)(i%320),y=(long)(i/320)-300;
            ++count;
            if(x<minx) minx=x; if(x>maxx) maxx=x; if(y<miny) miny=y; if(y>maxy) maxy=y;
        }
        printf("{\"case\":%lu,\"colour\":%lu,\"points\":[",n,colour);
        for(i=0;i<sides;++i) printf("%s[%d,%d]",i?",":"",points[i][0],points[i][1]);
        printf("],\"written\":%lu,\"bounds\":[%ld,%ld,%ld,%ld],\"pixels_fnv1a64\":\"%016llx\"}\n",
               count,count?minx:0,count?miny:0,count?maxx:0,count?maxy:0,
               (unsigned long long)fingerprint(buffer,sizeof(buffer)));
    }
    if (fflush(stdout)) fail("cannot flush span raster vectors");
    return 0;
}

/* Thrust limits. thrust-limits drives the original F598, F599 and F600 on a
   zeroed object whose limit words (+0xbc/+0xbe, +0xc0/+0xc2, +0xc4/+0xc6) and
   request dwords are synthetic (in range, beyond a limit, negative, zero), and
   prints the returned dwords and the words stored at +0xb6/+0xb8/+0xba. */
extern void ref_thrust_request(unsigned char *object,const long *request,long *out);
extern void ref_thrust_clamp(unsigned char *object,const long *request,long *out);
extern void ref_thrust_scale(unsigned char *object,const long *request,long *out);
static int thrust_limit_vectors(void)
{
    static unsigned char object[0x200];
    long request[3],out[3];
    short limits[6];
    unsigned long n,i,fn;
    fprintf(stderr,"JJFFE thrust-limits-v1: original F598, F599 and F600 on a zeroed object with synthetic limit words and requests; the result dwords and the words stored at +0xb6/+0xb8/+0xba.\n");
    texture_rng=0x27d4eb2fUL;
    for(n=0;n<4096;++n) {
        fn=n%3;
        for(i=0;i<3;++i) {
            limits[2*i]=(short)texture_random(0,(n&16)?32767:4000);
            limits[2*i+1]=(short)-texture_random(0,(n&32)?32767:4000);
            if(n%11==0) limits[2*i]=limits[2*i+1]=0;
        }
        for(i=0;i<3;++i) {
            switch(texture_random(0,5)) {
            case 0: request[i]=0; break;
            case 1: request[i]=texture_random(-100,100); break;
            case 2: request[i]=texture_random(-40000,40000); break;
            case 3: request[i]=texture_random(-4000000,4000000); break;
            case 4: request[i]=limits[2*i]+texture_random(-2,2); break;
            default: request[i]=limits[2*i+1]+texture_random(-2,2); break;
            }
        }
        memset(object,0,sizeof(object));
        for(i=0;i<3;++i) {
            *(short *)(object + 0xbc + 4*i)=limits[2*i];
            *(short *)(object + 0xbe + 4*i)=limits[2*i+1];
        }
        out[0]=out[1]=out[2]=0x5a5a5a5aL;
        if(fn==0) ref_thrust_request(object,request,out);
        else if(fn==1) ref_thrust_clamp(object,request,out);
        else ref_thrust_scale(object,request,out);
        printf("{\"case\":%lu,\"function\":%lu,\"limits\":[%d,%d,%d,%d,%d,%d],\"request\":[%ld,%ld,%ld],\"out\":[%ld,%ld,%ld],\"words\":[%u,%u,%u]}\n",
               n,598+fn,limits[0],limits[1],limits[2],limits[3],limits[4],limits[5],
               request[0],request[1],request[2],out[0],out[1],out[2],
               *(unsigned short *)(object+0xb6),*(unsigned short *)(object+0xb8),*(unsigned short *)(object+0xba));
    }
    if (fflush(stdout)) fail("cannot flush thrust limit vectors");
    return 0;
}

/* Motion. accelerate drives the original F601 (velocity from thrust over a
   time step) on a zeroed object with a synthetic orientation, thrust words,
   fractional accumulators, velocity and caps, over time steps in all three
   regimes (below 0x4014 after >> 3, up to 0x7fff, and beyond it). */
extern void ref_accelerate(unsigned char *object,unsigned long tics);
extern void ref_accelerate_uncapped(unsigned char *object,unsigned long tics);
static void put32(unsigned char *object,unsigned long offset,long value) { memcpy(object+offset,&value,4); }
static long get32(const unsigned char *object,unsigned long offset) { long v; int w; memcpy(&w,object+offset,4); v=w; return v; }
static int accelerate_vectors(void)
{
    static unsigned char object[0x200];
    unsigned long n,i,tics;
    long cap;
    fprintf(stderr,"JJFFE accelerate-v1: original F601 and F602 (without the caps) on a zeroed object with a synthetic orientation (+0x0), thrust words (+0xb6), fraction words (+0xec), velocity (+0x8c) and caps (+0xf2) over time steps in the small, medium and large regimes; the velocity dwords and fraction words after.\n");
    texture_rng=0x165667b1UL;
    for(n=0;n<4096;++n) {
        switch(n%4) {
        case 0: tics=(unsigned long)texture_random(0,0x20000); break;
        case 1: tics=(unsigned long)texture_random(0x20000,0x40000); break;
        case 2: tics=(unsigned long)texture_random(0x40000,0x400000); break;
        default: tics=(unsigned long)texture_random(0,0x7fffffff); break;
        }
        memset(object,0,sizeof(object));
        for(i=0;i<9;++i) put32(object,4*i,(long)(((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff)));
        for(i=0;i<3;++i) {
            short w=(short)texture_random(-20000,20000);
            if(n%5==i) w=0;
            memcpy(object+0xb6+2*i,&w,2);
            w=(short)texture_random(0,0xffff);
            memcpy(object+0xec+2*i,&w,2);
            put32(object,0x8c+4*i,(long)texture_random(-1000000,1000000));
            switch(texture_random(0,3)) {
            case 0: cap=texture_random(0,5000); break;
            case 1: cap=-texture_random(0,5000); break;
            case 2: cap=texture_random(-2000000,2000000); break;
            default: cap=0; break;
            }
            put32(object,0xf2+4*i,cap);
        }
        {
            unsigned char before[0x200];
            memcpy(before,object,sizeof(before));
            if((n/4)&1) ref_accelerate_uncapped(object,tics); else ref_accelerate(object,tics);
            printf("{\"case\":%lu,\"function\":%u,\"tics\":%lu,\"matrix\":[",n,601u+(unsigned)((n/4)&1),tics);
            for(i=0;i<9;++i) printf("%s%ld",i?",":"",get32(before,4*i));
            printf("],\"thrust\":[");
            for(i=0;i<3;++i) printf("%s%d",i?",":"",*(short *)(before+0xb6+2*i));
            printf("],\"fraction\":[");
            for(i=0;i<3;++i) printf("%s%u",i?",":"",*(unsigned short *)(before+0xec+2*i));
            printf("],\"velocity\":[");
            for(i=0;i<3;++i) printf("%s%ld",i?",":"",get32(before,0x8c+4*i));
            printf("],\"cap\":[");
            for(i=0;i<3;++i) printf("%s%ld",i?",":"",get32(before,0xf2+4*i));
            printf("],\"after\":{\"velocity\":[");
            for(i=0;i<3;++i) printf("%s%ld",i?",":"",get32(object,0x8c+4*i));
            printf("],\"fraction\":[");
            for(i=0;i<3;++i) printf("%s%u",i?",":"",*(unsigned short *)(object+0xec+2*i));
            printf("]}}\n");
        }
    }
    if (fflush(stdout)) fail("cannot flush accelerate vectors");
    return 0;
}

/* Gravity. gravity drives the original F603 (the parent body's pull on an
   object in its frame of reference: relative position cut to 13 bits, ffp
   length cubed, the mass divided by it, times the time step) on a zeroed object
   whose parent byte (+0x56) indexes a synthetic object list holding the
   parent's mass dword (+0xa4), over random positions, masses and time steps. */
extern void ref_gravity(unsigned char *object,unsigned char *list,long time);
extern unsigned long ref_gravity_x, ref_gravity_y, ref_gravity_z;
static long long get64(const unsigned char *object,unsigned long offset);
static int gravity_vectors(void)
{
    static unsigned char object[0x200], list[0x800];
    unsigned long n,i;
    long time;
    fprintf(stderr,"JJFFE gravity-v1: original F603 on a zeroed object with a parent byte (+0x56) into a synthetic object list holding the parent's ffp mass dword (+0xa4), a 64-bit relative position (+0x3e), velocity (+0x8c) and fraction words (+0xec) over positions of every magnitude and time steps of both signs; the velocity dwords, fraction words and the truncated position left in DATA_008708..DATA_008710 after.\n");
    texture_rng=0x2545f491UL;
    for(n=0;n<4096;++n) {
        unsigned long long coordinate;
        unsigned int shift,parent;
        long long position[3];
        short mantissa,exponent;
        memset(object,0,sizeof(object));
        memset(list,0,sizeof(list));
        parent=(n%16==0)?0:(unsigned int)texture_random(1,3);
        object[0x56]=(unsigned char)parent;
        for(i=0;i<3;++i) {
            shift=(unsigned int)texture_random(0,58);
            coordinate=(((unsigned long long)(unsigned long)texture_random(0,0x7fffffff)<<32)|(unsigned long long)(unsigned long)texture_random(0,0x7fffffff))>>shift;
            position[i]=(long long)coordinate;
            if(texture_random(0,1)) position[i]=-position[i];
            if(n%23==0) position[i]=0;
            memcpy(object + 0x3e + 8*i,&position[i],8);
            put32(object,0x8c+4*i,(long)texture_random(-1000000,1000000));
            {unsigned short w=(unsigned short)texture_random(0,0xffff);memcpy(object+0xec+2*i,&w,2);}
        }
        mantissa=(n%29==0)?0:(short)texture_random(0x4000,0x7fff);
        exponent=(short)texture_random(-8,70);
        memcpy(list+parent*0x152+0x74+0xa4,&mantissa,2);
        memcpy(list+parent*0x152+0x74+0xa6,&exponent,2);
        switch(texture_random(0,3)) {
        case 0: time=texture_random(-2000,2000); break;
        case 1: time=texture_random(-100000,100000); break;
        case 2: time=texture_random(-0x7fffffff,0x7fffffff); break;
        default: time=0; break;
        }
        {
            unsigned char before[0x200];
            memcpy(before,object,sizeof(before));
            ref_gravity_x=ref_gravity_y=ref_gravity_z=0x5a5a5a5aUL;
            ref_gravity(object,list,time);
            printf("{\"case\":%lu,\"parent\":%u,\"mass\":[%d,%d],\"time\":%ld,\"position\":[%lld,%lld,%lld],\"velocity\":[",n,parent,(int)mantissa,(int)exponent,time,get64(before,0x3e),get64(before,0x46),get64(before,0x4e));
            for(i=0;i<3;++i) printf("%s%ld",i?",":"",get32(before,0x8c+4*i));
            printf("],\"fraction\":[");
            for(i=0;i<3;++i) printf("%s%u",i?",":"",*(unsigned short *)(before+0xec+2*i));
            printf("],\"after\":{\"velocity\":[");
            for(i=0;i<3;++i) printf("%s%ld",i?",":"",get32(object,0x8c+4*i));
            printf("],\"fraction\":[");
            for(i=0;i<3;++i) printf("%s%u",i?",":"",*(unsigned short *)(object+0xec+2*i));
            printf("],\"truncated\":[%ld,%ld,%ld]}}\n",(long)(int)ref_gravity_x,(long)(int)ref_gravity_y,(long)(int)ref_gravity_z);
        }
    }
    if (fflush(stdout)) fail("cannot flush gravity vectors");
    return 0;
}

/* Frames of reference. frames drives the original F590 (distance between two
   bodies through the cached absolute chain F1677/F1678), F604 and F605 (move
   an object to its parent's parent frame, or into a given body's frame) and
   F606 (choose the frame) on a synthetic object list of seven bodies in a tree
   (parent below child), with random positions, velocities, radii, live flags
   and cache stamps. Every body's parent byte, position, velocity, stamp and
   cache after, and F590's vector and shift. */
extern void ref_frame_distance(unsigned char *a,unsigned char *b,long *vector,long *shift);
extern void ref_frame_enter_parent(unsigned char *object,unsigned char *parent);
extern void ref_frame_enter(unsigned char *object,unsigned char *list,long index);
extern void ref_frame_select(unsigned char *object,unsigned char *list);
extern unsigned char *ref_list_pointer;
extern unsigned char ref_frame_stamp;
static long long get64(const unsigned char *object,unsigned long offset);
static int frames_vectors(void)
{
    static unsigned char list[0x74+0x152*0x73+0x100];
    unsigned long n,i,k,a,b,pick;
    long vector[3]={0,0,0},shift=0;
    fprintf(stderr,"JJFFE frames-v1: original F590, F604, F605 and F606 on a synthetic object list of seven bodies in a tree (positions, velocities, radii +0xda, live flags, frame stamps and caches random; stations only locked), the cached absolute chain F1677/F1678 included; every body's parent byte, position, velocity, stamp and cache after, and F590's vector and shift.\n");
    texture_rng=0x3c6ef372UL;
    for(n=0;n<4096;++n) {
        unsigned char *o[8];
        long long position;
        unsigned int scale;
        memset(list,0,sizeof(list));
        ref_list_pointer=list;
        ref_frame_stamp=(unsigned char)texture_random(0,3);
        for(k=0;k<8;++k) o[k]=list+0x74+0x152*k;
        for(k=1;k<8;++k) {
            long radius;
            o[k][0x56]=(unsigned char)(k==1?0:texture_random(0,(long)k-1));
            o[k][0x86]=(unsigned char)k;
            o[k][0x87]=(unsigned char)(texture_random(0,5)==0?1:texture_random(2,9));
            if(o[k][0x87]==1) o[k][0x14c]=0x20;
            radius=texture_random(0,40);
            memcpy(o[k]+0xda,&radius,2);
            o[k][0x25]=(unsigned char)texture_random(0,3);
            list[k]=(unsigned char)(texture_random(0,3)?0x10:0);
            for(i=0;i<3;++i) {
                scale=(unsigned int)texture_random(0,56);
                position=(long long)((((unsigned long long)(unsigned long)texture_random(0,0x7fffffff)<<32)|(unsigned long long)(unsigned long)texture_random(0,0x7fffffff))>>scale);
                if(texture_random(0,1)) position=-position;
                memcpy(o[k] + 0x3e + 8*i,&position,8);
                position=(long long)((((unsigned long long)(unsigned long)texture_random(0,0x7fffffff)<<32)|(unsigned long long)(unsigned long)texture_random(0,0x7fffffff))>>scale);
                memcpy(o[k]+0x26+8*i,&position,8);
                put32(o[k],0x8c+4*i,(long)texture_random(-100000,100000));
            }
        }
        a=(unsigned long)texture_random(1,7);
        do b=(unsigned long)texture_random(1,7); while(b==a);
        pick=n%4;
        {
            unsigned char before[8][0x152];
            for(k=0;k<8;++k) memcpy(before[k],o[k],0x152);
            switch(pick) {
            case 0: ref_frame_distance(o[a],o[b],vector,&shift); break;
            case 1: ref_frame_enter_parent(o[a],o[b]); break;
            case 2: ref_frame_enter(o[a],list,(long)b); break;
            default: ref_frame_select(o[a],list); break;
            }
            printf("{\"case\":%lu,\"function\":%u,\"stamp\":%u,\"a\":%lu,\"b\":%lu,\"live\":[",n,590u+(unsigned)(pick==0?0:pick+13),(unsigned)ref_frame_stamp,a,b);
            for(k=1;k<8;++k) printf("%s%u",k>1?",":"",list[k]&0x10?1u:0u);
            printf("],\"bodies\":[");
            for(k=1;k<8;++k) {
                printf("%s{\"parent\":%u,\"kind\":%u,\"radius\":%d,\"stamp\":%u,\"position\":[%lld,%lld,%lld],\"velocity\":[%ld,%ld,%ld],\"cache\":[%lld,%lld,%lld],\"after\":{\"parent\":%u,\"stamp\":%u,\"position\":[%lld,%lld,%lld],\"velocity\":[%ld,%ld,%ld],\"cache\":[%lld,%lld,%lld]}}",
                    k>1?",":"",before[k][0x56],before[k][0x87],(int)*(short *)(before[k]+0xda),before[k][0x25],
                    get64(before[k],0x3e),get64(before[k],0x46),get64(before[k],0x4e),
                    get32(before[k],0x8c),get32(before[k],0x90),get32(before[k],0x94),
                    get64(before[k],0x26),get64(before[k],0x2e),get64(before[k],0x36),
                    o[k][0x56],o[k][0x25],get64(o[k],0x3e),get64(o[k],0x46),get64(o[k],0x4e),
                    get32(o[k],0x8c),get32(o[k],0x90),get32(o[k],0x94),
                    get64(o[k],0x26),get64(o[k],0x2e),get64(o[k],0x36));
            }
            printf("],\"vector\":[%ld,%ld,%ld],\"shift\":%ld}\n",pick==0?vector[0]:0,pick==0?vector[1]:0,pick==0?vector[2]:0,pick==0?shift:0);
        }
    }
    if (fflush(stdout)) fail("cannot flush frames vectors");
    return 0;
}

/* Orientation. orient drives the original F585 (angular rates spent in F584's
   steps through F611's small rotations, with F1520 renormalising when the drift
   counter at +0x24 runs out) on a zeroed object with a synthetic orientation
   (near identity or random), drift counter, rates, model dword, craft byte and
   autopilot rates, and prints the object before and after plus the scanner
   rotation word (DATA_008933) and the player's craft number (DATA_008857). */
extern void ref_orient(unsigned char *object,long time,long aiflag,long aitime);
extern unsigned long ref_player_craft;
extern unsigned short ref_scanner;
static int orient_vectors(void)
{
    static unsigned char object[0x200];
    unsigned char before[0x200];
    unsigned long n,i;
    long time,aiflag,aitime;
    unsigned long model;
    fprintf(stderr,"JJFFE orient-v1: original F585 (F584, F611, F1520/F1519/F1552) on a zeroed object with a synthetic orientation (+0x0), drift counter (+0x24), rates (+0xb0), model dword (+0x82), craft byte (+0x86) and autopilot rates (+0x110); the object before and after, with the scanner word (DATA_008933) and player craft (DATA_008857).\n");
    texture_rng=0x9e3779b1UL;
    for(n=0;n<4096;++n) {
        memset(object,0,sizeof(object));
        if(n&1) {
            for(i=0;i<3;++i) put32(object,12*i+4*i,0x7fffffffL-texture_random(0,100000));
            for(i=0;i<9;++i) if(i%4) put32(object,4*i,(long)texture_random(-3000000,3000000));
        } else for(i=0;i<9;++i) put32(object,4*i,(long)(((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff)));
        object[0x24]=(unsigned char)(signed char)texture_random(-2,0x40);
        for(i=0;i<3;++i) {
            short rate;
            switch(texture_random(0,4)) {
            case 0: rate=0; break;
            case 1: rate=(short)texture_random(-14,14); break;
            case 2: rate=(short)texture_random(-3000,3000); break;
            default: rate=(short)texture_random(-32768,32767); break;
            }
            memcpy(object+0xb0+2*i,&rate,2);
            rate=(short)texture_random(-30000,30000);
            memcpy(object+0x110+2*i,&rate,2);
        }
        model=(unsigned long)texture_random(0,0x80);
        if(texture_random(0,7)==0) model|=(unsigned long)texture_random(0,0xffff)<<16;
        memcpy(object+0x82,&model,4);
        object[0x86]=(unsigned char)texture_random(0,3);
        ref_player_craft=(unsigned long)texture_random(0,3);
        ref_scanner=(unsigned short)texture_random(0,0xffff);
        switch(texture_random(0,4)) {
        case 0: time=texture_random(0,200); break;
        case 1: time=texture_random(0,4000); break;
        case 2: time=texture_random(0,40000); break;
        case 3: time=texture_random(-5000,0); break;
        default: time=texture_random(0,0x7fffffffL>>8); break;
        }
        aiflag=(long)texture_random(0,1);
        aitime=texture_random(-100,70000);
        memcpy(before,object,sizeof(before));
        {
            unsigned long player=ref_player_craft; unsigned short scanner=ref_scanner;
            ref_orient(object,time,aiflag,aitime);
            printf("{\"case\":%lu,\"matrix\":[",n);
            for(i=0;i<9;++i) printf("%s%ld",i?",":"",get32(before,4*i));
            printf("],\"drift\":%d,\"rate\":[",(int)(signed char)before[0x24]);
            for(i=0;i<3;++i) printf("%s%d",i?",":"",*(short *)(before+0xb0+2*i));
            printf("],\"autopilot\":[");
            for(i=0;i<3;++i) printf("%s%d",i?",":"",*(short *)(before+0x110+2*i));
            printf("],\"model\":%lu,\"craft\":%u,\"player\":%lu,\"scanner\":%u,\"time\":%ld,\"ai\":%ld,\"ai_time\":%ld,\"after\":{\"matrix\":[",
                   model,before[0x86],player,scanner,time,aiflag,aitime);
            for(i=0;i<9;++i) printf("%s%ld",i?",":"",get32(object,4*i));
            printf("],\"drift\":%d,\"rate\":[",(int)(signed char)object[0x24]);
            for(i=0;i<3;++i) printf("%s%d",i?",":"",*(short *)(object+0xb0+2*i));
            printf("],\"scanner\":%u}}\n",ref_scanner);
        }
    }
    if (fflush(stdout)) fail("cannot flush orient vectors");
    return 0;
}

/* The player's free-flight update. player-orient drives the original F1041 (two
   F585 orientation updates, the frame-of-reference countdown, then the take-off,
   engines-off (F1043) or set-speed (F607) thrust request through F599) in free
   space: no parent frame (+0x56 zero), an altitude beyond the planet range and
   an empty object list. The globals it reads are set per case; the object
   before and after is printed. */
extern void ref_player_orient(unsigned char *object);
extern unsigned long ref_pl_frame_tics,ref_pl_altitude,ref_pl_set_speed;
extern unsigned char *ref_pl_object_list;
extern unsigned char ref_pl_mode,ref_pl_flags,ref_pl_combat,ref_pl_keys[0x60];
extern long ref_pl_combat_velocity[3];
static int player_orient_vectors(void)
{
    static unsigned char object[0x400];
    static unsigned char list[0x20000];
    unsigned char before[0x400];
    unsigned long n,i,model;
    static const int key_offsets[10]={0x1c,0x36,0x48,0x4c,0x4b,0x4d,0x4f,0x51,0x49,0x47};
    int pressed[10];
    fprintf(stderr,"JJFFE player-orient-v1: original F1041 (F585 twice, F606 countdown, F599 with the take-off, engines-off F1043 or set-speed F607 request) in free space: no parent frame, altitude beyond the planet range, an empty object list; the object before and after, with the globals set per case.\n");
    texture_rng=0x7a2e1c3bUL;
    for(n=0;n<4096;++n) {
        memset(object,0,sizeof(object));memset(list,0,sizeof(list));
        ref_pl_object_list=list;
        if(n&1) {
            for(i=0;i<3;++i) put32(object,12*i+4*i,0x7fffffffL-texture_random(0,100000));
            for(i=0;i<9;++i) if(i%4) put32(object,4*i,(long)texture_random(-3000000,3000000));
        } else for(i=0;i<9;++i) put32(object,4*i,(long)(((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff)));
        object[0x24]=(unsigned char)(signed char)texture_random(-2,0x40);
        for(i=0;i<3;++i) {
            short rate,lim;
            switch(texture_random(0,3)) {
            case 0: rate=0; break;
            case 1: rate=(short)texture_random(-14,14); break;
            default: rate=(short)texture_random(-4000,4000); break;
            }
            memcpy(object+0xb0+2*i,&rate,2);
            lim=(short)texture_random(0,(n&16)?32767:6000); memcpy(object + 0xbc + 4*i,&lim,2);
            lim=(short)-texture_random(0,(n&32)?32767:6000); memcpy(object + 0xbe + 4*i,&lim,2);
            put32(object,0x8c+4*i,(long)texture_random(-3000000,3000000));
            ref_pl_combat_velocity[i]=(long)texture_random(-3000000,3000000);
        }
        model=(unsigned long)texture_random(0,0x80);
        memcpy(object+0x82,&model,4);
        object[0x86]=(unsigned char)texture_random(0,3);
        object[0x14d]=(unsigned char)(signed char)texture_random(-1,0xd);
        ref_player_craft=(unsigned long)texture_random(0,3);
        ref_scanner=(unsigned short)texture_random(0,0xffff);
        ref_pl_frame_tics=(unsigned long)texture_random(0,(n&64)?0x80000:0x4000);
        ref_pl_altitude=0x2625a01UL+(unsigned long)texture_random(0,0x7fffffff-0x2625a01);
        ref_pl_mode=(unsigned char)(texture_random(0,5)==0?0x24:0);
        ref_pl_flags=(unsigned char)texture_random(0,1);
        ref_pl_combat=(unsigned char)(texture_random(0,1)?2:0);
        switch(texture_random(0,3)) {
        case 0: ref_pl_set_speed=0; break;
        case 1: ref_pl_set_speed=(unsigned long)texture_random(-5000,5000); break;
        case 2: ref_pl_set_speed=(unsigned long)texture_random(-3000000,3000000); break;
        default: ref_pl_set_speed=(unsigned long)texture_random(-100000000,100000000); break;
        }
        memset(ref_pl_keys,0x80,sizeof(ref_pl_keys));
        for(i=0;i<10;++i) {pressed[i]=texture_random(0,3)==0;if(pressed[i]) ref_pl_keys[key_offsets[i]]=0;}
        memcpy(before,object,sizeof(before));
        {
            unsigned long player=ref_player_craft; unsigned short scanner=ref_scanner;
            ref_player_orient(object);
            printf("{\"case\":%lu,\"matrix\":[",n);
            for(i=0;i<9;++i) printf("%s%ld",i?",":"",get32(before,4*i));
            printf("],\"drift\":%d,\"rate\":[",(int)(signed char)before[0x24]);
            for(i=0;i<3;++i) printf("%s%d",i?",":"",*(short *)(before+0xb0+2*i));
            printf("],\"limits\":[");
            for(i=0;i<3;++i) printf("%s%d,%d",i?",":"",*(short *)(before + 0xbc + 4*i),*(short *)(before + 0xbe + 4*i));
            printf("],\"velocity\":[");
            for(i=0;i<3;++i) printf("%s%ld",i?",":"",get32(before,0x8c+4*i));
            printf("],\"model\":%lu,\"craft\":%u,\"player\":%lu,\"scanner\":%u,\"counter\":%d,\"tics\":%lu,\"altitude\":%ld,\"mode\":%u,\"flags\":%u,\"combat\":%u,\"combat_velocity\":[%ld,%ld,%ld],\"set_speed\":%ld,\"keys\":[",
                   model,before[0x86],player,scanner,(int)(signed char)before[0x14d],ref_pl_frame_tics,(long)ref_pl_altitude,ref_pl_mode,ref_pl_flags,ref_pl_combat,
                   ref_pl_combat_velocity[0],ref_pl_combat_velocity[1],ref_pl_combat_velocity[2],(long)ref_pl_set_speed);
            for(i=0;i<10;++i) printf("%s%d",i?",":"",pressed[i]);
            printf("],\"after\":{\"matrix\":[");
            for(i=0;i<9;++i) printf("%s%ld",i?",":"",get32(object,4*i));
            printf("],\"drift\":%d,\"rate\":[",(int)(signed char)object[0x24]);
            for(i=0;i<3;++i) printf("%s%d",i?",":"",*(short *)(object+0xb0+2*i));
            printf("],\"thrust\":[");
            for(i=0;i<3;++i) printf("%s%d",i?",":"",*(short *)(object+0xb6+2*i));
            printf("],\"cap\":[");
            for(i=0;i<3;++i) printf("%s%ld",i?",":"",get32(object,0xf2+4*i));
            printf("],\"counter\":%d,\"scanner\":%u}}\n",(int)(signed char)object[0x14d],ref_scanner);
        }
    }
    if (fflush(stdout)) fail("cannot flush player-orient vectors");
    return 0;
}

/* Position. advance drives the original F1512/F1511 (velocity over a 64-bit
   time added to a 64-bit position) on a zeroed object with a synthetic position
   (+0x3e), velocity (+0x8c) and 64-bit time (high dword first, as F1045 passes
   DATA_008869 and DATA_008868), over small, large, huge and negative times. */
extern void ref_advance(unsigned char *object,unsigned long high,unsigned long low);
static void put64(unsigned char *object,unsigned long offset,unsigned long long value) { memcpy(object+offset,&value,8); }
static long long get64(const unsigned char *object,unsigned long offset) { long long v; memcpy(&v,object+offset,8); return v; }
static int advance_vectors(void)
{
    static unsigned char object[0x200];
    unsigned long n,i,high,low;
    unsigned long long time;
    unsigned char before[0x200];
    fprintf(stderr,"JJFFE advance-v1: original F1512/F1511 on a zeroed object with a synthetic position (+0x3e, three 64-bit values), velocity (+0x8c) and 64-bit time; the position before and after.\n");
    texture_rng=0x1f83d9abUL;
    for(n=0;n<4096;++n) {
        memset(object,0,sizeof(object));
        for(i=0;i<3;++i) {
            put64(object,0x3e + 8*i,((unsigned long long)(((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff))<<16)^(unsigned long long)texture_random(0,0xffff));
            if(n%7==i) put64(object,0x3e + 8*i,0);
            switch(texture_random(0,2)) {
            case 0: put32(object,0x8c+4*i,(long)texture_random(-2000,2000)); break;
            case 1: put32(object,0x8c+4*i,(long)texture_random(-3000000,3000000)); break;
            default: put32(object,0x8c+4*i,(long)(((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff))); break;
            }
        }
        switch(n%6) {
        case 0: time=(unsigned long long)texture_random(0,100000); break;
        case 1: time=(unsigned long long)texture_random(0,0x7fffffff); break;
        case 2: time=((unsigned long long)texture_random(0,0x3ff)<<32)|(unsigned long long)(((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff)); break;
        case 3: time=(unsigned long long)(-(long long)texture_random(0,100000)); break;
        case 4: time=(unsigned long long)(-(long long)(((unsigned long long)texture_random(0,0x7fffff)<<8)|(unsigned long long)texture_random(0,255))); break;
        default: time=((unsigned long long)(((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff))<<32)|(unsigned long long)(((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff)); break;
        }
        high=(unsigned long)(time>>32);low=(unsigned long)(time&0xffffffffUL);
        memcpy(before,object,sizeof(before));
        ref_advance(object,high,low);
        printf("{\"case\":%lu,\"time\":%lld,\"position\":[",n,(long long)time);
        for(i=0;i<3;++i) printf("%s%lld",i?",":"",get64(before,0x3e + 8*i));
        printf("],\"velocity\":[");
        for(i=0;i<3;++i) printf("%s%ld",i?",":"",get32(before,0x8c+4*i));
        printf("],\"after\":[");
        for(i=0;i<3;++i) printf("%s%lld",i?",":"",get64(object,0x3e + 8*i));
        printf("]}\n");
    }
    if (fflush(stdout)) fail("cannot flush advance vectors");
    return 0;
}

static int flight_step_vectors(void)
{
    static unsigned char object[0x400];
    static unsigned char list[0x20000];
    unsigned char before[0x400];
    unsigned long n,i,model,time_low,time_high;
    long long time64;
    unsigned long long start;
    static const int key_offsets[10]={0x1c,0x36,0x48,0x4c,0x4b,0x4d,0x4f,0x51,0x49,0x47};
    int pressed[10];
    fprintf(stderr,"JJFFE flight-step-v1: one free-flight tick of the original in free space: F1041, then F602 (engines off) or F601 for the velocity over the low time dword, then F1512 for the position over the 64-bit time (DATA_008869:DATA_008868); the object before and after.\n");
    texture_rng=0x7a2e1c3bUL;
    for(n=0;n<4096;++n) {
        memset(object,0,sizeof(object));memset(list,0,sizeof(list));
        ref_pl_object_list=list;
        if(n&1) {
            for(i=0;i<3;++i) put32(object,12*i+4*i,0x7fffffffL-texture_random(0,100000));
            for(i=0;i<9;++i) if(i%4) put32(object,4*i,(long)texture_random(-3000000,3000000));
        } else for(i=0;i<9;++i) put32(object,4*i,(long)(((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff)));
        object[0x24]=(unsigned char)(signed char)texture_random(-2,0x40);
        for(i=0;i<3;++i) {
            short rate,lim;
            switch(texture_random(0,3)) {
            case 0: rate=0; break;
            case 1: rate=(short)texture_random(-14,14); break;
            default: rate=(short)texture_random(-4000,4000); break;
            }
            memcpy(object+0xb0+2*i,&rate,2);
            lim=(short)texture_random(0,(n&16)?32767:6000); memcpy(object + 0xbc + 4*i,&lim,2);
            lim=(short)-texture_random(0,(n&32)?32767:6000); memcpy(object + 0xbe + 4*i,&lim,2);
            put32(object,0x8c+4*i,(long)texture_random(-3000000,3000000));
            ref_pl_combat_velocity[i]=(long)texture_random(-3000000,3000000);
        }
        model=(unsigned long)texture_random(0,0x80);
        memcpy(object+0x82,&model,4);
        object[0x86]=(unsigned char)texture_random(0,3);
        object[0x14d]=(unsigned char)(signed char)texture_random(-1,0xd);
        ref_player_craft=(unsigned long)texture_random(0,3);
        ref_scanner=(unsigned short)texture_random(0,0xffff);
        ref_pl_frame_tics=(unsigned long)texture_random(0,(n&64)?0x80000:0x4000);
        ref_pl_altitude=0x2625a01UL+(unsigned long)texture_random(0,0x7fffffff-0x2625a01);
        ref_pl_mode=(unsigned char)(texture_random(0,5)==0?0x24:0);
        ref_pl_flags=(unsigned char)texture_random(0,1);
        ref_pl_combat=(unsigned char)(texture_random(0,1)?2:0);
        switch(texture_random(0,3)) {
        case 0: ref_pl_set_speed=0; break;
        case 1: ref_pl_set_speed=(unsigned long)texture_random(-5000,5000); break;
        case 2: ref_pl_set_speed=(unsigned long)texture_random(-3000000,3000000); break;
        default: ref_pl_set_speed=(unsigned long)texture_random(-100000000,100000000); break;
        }
        memset(ref_pl_keys,0x80,sizeof(ref_pl_keys));
        for(i=0;i<10;++i) {pressed[i]=texture_random(0,3)==0;if(pressed[i]) ref_pl_keys[key_offsets[i]]=0;}
        switch(texture_random(0,3)) {
        case 0: time64=(long long)texture_random(0,2000); break;
        case 1: time64=(long long)texture_random(0,100000); break;
        case 2: time64=(long long)texture_random(0,0x7fffffff); break;
        default: time64=((long long)texture_random(0,0xff)<<32)|(long long)texture_random(0,0x7fffffff); break;
        }
        time_low=(unsigned long)(time64&0xffffffffLL);time_high=(unsigned long)(time64>>32);
        for(i=0;i<3;++i) {
            start=((unsigned long long)(((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff))<<16)^(unsigned long long)texture_random(0,0xffff);
            memcpy(object + 0x3e + 8*i,&start,8);
        }
        memcpy(before,object,sizeof(before));
        {
            unsigned long player=ref_player_craft; unsigned short scanner=ref_scanner;
            ref_player_orient(object);
            if(ref_pl_flags&1) ref_accelerate_uncapped(object,time_low); else ref_accelerate(object,time_low);
            ref_advance(object,time_high,time_low);
            printf("{\"case\":%lu,\"time\":%lld,\"position\":[%lld,%lld,%lld],\"matrix\":[",n,time64,get64(before,0x3e),get64(before,0x46),get64(before,0x4e));
            for(i=0;i<9;++i) printf("%s%ld",i?",":"",get32(before,4*i));
            printf("],\"drift\":%d,\"rate\":[",(int)(signed char)before[0x24]);
            for(i=0;i<3;++i) printf("%s%d",i?",":"",*(short *)(before+0xb0+2*i));
            printf("],\"limits\":[");
            for(i=0;i<3;++i) printf("%s%d,%d",i?",":"",*(short *)(before + 0xbc + 4*i),*(short *)(before + 0xbe + 4*i));
            printf("],\"velocity\":[");
            for(i=0;i<3;++i) printf("%s%ld",i?",":"",get32(before,0x8c+4*i));
            printf("],\"model\":%lu,\"craft\":%u,\"player\":%lu,\"scanner\":%u,\"counter\":%d,\"tics\":%lu,\"altitude\":%ld,\"mode\":%u,\"flags\":%u,\"combat\":%u,\"combat_velocity\":[%ld,%ld,%ld],\"set_speed\":%ld,\"keys\":[",
                   model,before[0x86],player,scanner,(int)(signed char)before[0x14d],ref_pl_frame_tics,(long)ref_pl_altitude,ref_pl_mode,ref_pl_flags,ref_pl_combat,
                   ref_pl_combat_velocity[0],ref_pl_combat_velocity[1],ref_pl_combat_velocity[2],(long)ref_pl_set_speed);
            for(i=0;i<10;++i) printf("%s%d",i?",":"",pressed[i]);
            printf("],\"after\":{\"matrix\":[");
            for(i=0;i<9;++i) printf("%s%ld",i?",":"",get32(object,4*i));
            printf("],\"drift\":%d,\"rate\":[",(int)(signed char)object[0x24]);
            for(i=0;i<3;++i) printf("%s%d",i?",":"",*(short *)(object+0xb0+2*i));
            printf("],\"thrust\":[");
            for(i=0;i<3;++i) printf("%s%d",i?",":"",*(short *)(object+0xb6+2*i));
            printf("],\"cap\":[");
            for(i=0;i<3;++i) printf("%s%ld",i?",":"",get32(object,0xf2+4*i));
            printf("],\"counter\":%d,\"scanner\":%u,\"velocity\":[%ld,%ld,%ld],\"position\":[%lld,%lld,%lld]}}\n",(int)(signed char)object[0x14d],ref_scanner,get32(object,0x8c),get32(object,0x90),get32(object,0x94),get64(object,0x3e),get64(object,0x46),get64(object,0x4e));
        }
    }
    if (fflush(stdout)) fail("cannot flush flight-step vectors");
    return 0;
}

/* World step. world-step runs one whole tick of the original (F1041 with
   F585 twice, F603 gravity, F609 auto-roll, the F606 countdown and the thrust
   request, then F601/F602 and F1512) for the player as the last body of a
   synthetic seven-body tree, with a parent frame, an ffp mass on every body,
   altitudes either side of the auto-roll limit and random positions. Every
   body's parent, position, velocity, stamp and cache, the player's matrix,
   rates, thrust, caps, fractions, countdown, scanner, auto-roll rate and the
   gravity globals (DATA_008708..DATA_008710) after. */
static int world_step_vectors(void)
{
    static unsigned char list[0x20000];
    unsigned char *o[8];
    unsigned char *object;
    unsigned char before[8][0x152];
    unsigned long n,i,k,model,time_low,time_high,gravity_before[3];
    long long time64;
    static const int key_offsets[10]={0x1c,0x36,0x48,0x4c,0x4b,0x4d,0x4f,0x51,0x49,0x47};
    int pressed[10];
    fprintf(stderr,"JJFFE world-step-v1: one whole tick of the original (F1041 with F585 twice, F603 gravity, F609 auto-roll, the F606 countdown and the thrust request, then F601/F602 and F1512) for the player as the last body of a synthetic seven-body tree with ffp masses, altitudes either side of the auto-roll limit and random positions; every body's parent, position, velocity, stamp and cache, the player's matrix, rates, thrust, caps, fractions, countdown, scanner, auto-roll rate and the gravity globals after.\n");
    texture_rng=0x51ed270bUL;
    for(n=0;n<4096;++n) {
        memset(list,0,0x74+0x152*8+0x100);
        for(k=0;k<8;++k) o[k]=list+0x74+0x152*k;
        object=o[7];
        ref_pl_object_list=list;
        ref_list_pointer=list;
        ref_frame_stamp=(unsigned char)texture_random(0,3);
        for(k=1;k<8;++k) {
            long radius;
            short mantissa,exponent;
            long long position;
            unsigned int scale;
            o[k][0x56]=(unsigned char)(k==1?0:texture_random(0,(long)k-1));
            if(k<7) o[k][0x86]=(unsigned char)k;
            o[k][0x87]=(unsigned char)(texture_random(0,5)==0?1:texture_random(2,9));
            if(o[k][0x87]==1) o[k][0x14c]=0x20;
            radius=texture_random(0,40);
            memcpy(o[k]+0xda,&radius,2);
            mantissa=(texture_random(0,9)==0)?0:(short)texture_random(0x4000,0x7fff);
            exponent=(short)texture_random(-8,70);
            memcpy(o[k]+0xa4,&mantissa,2);
            memcpy(o[k]+0xa6,&exponent,2);
            o[k][0x25]=(unsigned char)texture_random(0,3);
            list[k]=(unsigned char)(texture_random(0,3)?0x10:0);
            for(i=0;i<3;++i) {
                scale=(unsigned int)texture_random(0,56);
                position=(long long)((((unsigned long long)(unsigned long)texture_random(0,0x7fffffff)<<32)|(unsigned long long)(unsigned long)texture_random(0,0x7fffffff))>>scale);
                if(texture_random(0,1)) position=-position;
                memcpy(o[k] + 0x3e + 8*i,&position,8);
                position=(long long)((((unsigned long long)(unsigned long)texture_random(0,0x7fffffff)<<32)|(unsigned long long)(unsigned long)texture_random(0,0x7fffffff))>>scale);
                memcpy(o[k] + 0x26 + 8*i,&position,8);
                put32(o[k],0x8c+4*i,(long)texture_random(-100000,100000));
            }
        }
        if(n&1) {
            for(i=0;i<3;++i) put32(object,12*i+4*i,0x7fffffffL-texture_random(0,100000));
            for(i=0;i<9;++i) if(i%4) put32(object,4*i,(long)texture_random(-3000000,3000000));
        } else for(i=0;i<9;++i) put32(object,4*i,(long)(((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff)));
        object[0x24]=(unsigned char)(signed char)texture_random(-2,0x40);
        for(i=0;i<3;++i) {
            short rate,lim;
            switch(texture_random(0,3)) {
            case 0: rate=0; break;
            case 1: rate=(short)texture_random(-14,14); break;
            default: rate=(short)texture_random(-4000,4000); break;
            }
            memcpy(object+0xb0+2*i,&rate,2);
            lim=(short)texture_random(0,(n&16)?32767:6000); memcpy(object + 0xbc + 4*i,&lim,2);
            lim=(short)-texture_random(0,(n&32)?32767:6000); memcpy(object + 0xbe + 4*i,&lim,2);
            ref_pl_combat_velocity[i]=(long)texture_random(-3000000,3000000);
        }
        model=(unsigned long)texture_random(0,0x80);
        memcpy(object+0x82,&model,4);
        object[0x86]=(unsigned char)texture_random(0,3);
        object[0x14d]=(unsigned char)(signed char)texture_random(-1,0xd);
        ref_player_craft=(unsigned long)texture_random(0,3);
        ref_scanner=(unsigned short)texture_random(0,0xffff);
        ref_pl_frame_tics=(unsigned long)texture_random(0,(n&64)?0x80000:0x4000);
        ref_pl_altitude=(unsigned long)(texture_random(0,1)?texture_random(-1000,0x2625a00):0x2625a01L+texture_random(0,0x7fffffff-0x2625a01));
        ref_pl_mode=(unsigned char)(texture_random(0,5)==0?0x24:0);
        ref_pl_flags=(unsigned char)texture_random(0,1);
        ref_pl_combat=(unsigned char)(texture_random(0,1)?2:0);
        switch(texture_random(0,3)) {
        case 0: ref_pl_set_speed=0; break;
        case 1: ref_pl_set_speed=(unsigned long)texture_random(-5000,5000); break;
        case 2: ref_pl_set_speed=(unsigned long)texture_random(-3000000,3000000); break;
        default: ref_pl_set_speed=(unsigned long)texture_random(-100000000,100000000); break;
        }
        memset(ref_pl_keys,0x80,sizeof(ref_pl_keys));
        for(i=0;i<10;++i) {pressed[i]=texture_random(0,3)==0;if(pressed[i]) ref_pl_keys[key_offsets[i]]=0;}
        switch(texture_random(0,3)) {
        case 0: time64=(long long)texture_random(0,2000); break;
        case 1: time64=(long long)texture_random(0,100000); break;
        case 2: time64=(long long)texture_random(0,0x7fffffff); break;
        default: time64=((long long)texture_random(0,0xff)<<32)|(long long)texture_random(0,0x7fffffff); break;
        }
        time_low=(unsigned long)(time64&0xffffffffLL);time_high=(unsigned long)(time64>>32);
        gravity_before[0]=ref_gravity_x=(unsigned long)texture_random(-2000,2000);
        gravity_before[1]=ref_gravity_y=(unsigned long)texture_random(-2000,2000);
        gravity_before[2]=ref_gravity_z=(unsigned long)texture_random(-2000,2000);
        for(k=0;k<8;++k) memcpy(before[k],o[k],0x152);
        {
            unsigned long player=ref_player_craft; unsigned short scanner=ref_scanner;
            ref_player_orient(object);
            if(ref_pl_flags&1) ref_accelerate_uncapped(object,time_low); else ref_accelerate(object,time_low);
            ref_advance(object,time_high,time_low);
            printf("{\"case\":%lu,\"time\":%lld,\"stamp\":%u,\"live\":[",n,time64,(unsigned)ref_frame_stamp);
            for(k=1;k<8;++k) printf("%s%u",k>1?",":"",list[k]&0x10?1u:0u);
            printf("],\"matrix\":[");
            for(i=0;i<9;++i) printf("%s%ld",i?",":"",get32(before[7],4*i));
            printf("],\"drift\":%d,\"rate\":[",(int)(signed char)before[7][0x24]);
            for(i=0;i<3;++i) printf("%s%d",i?",":"",*(short *)(before[7]+0xb0+2*i));
            printf("],\"limits\":[");
            for(i=0;i<3;++i) printf("%s%d,%d",i?",":"",*(short *)(before[7] + 0xbc + 4*i),*(short *)(before[7] + 0xbe + 4*i));
            printf("],\"model\":%lu,\"craft\":%u,\"player\":%lu,\"scanner\":%u,\"counter\":%d,\"tics\":%lu,\"altitude\":%ld,\"mode\":%u,\"flags\":%u,\"combat\":%u,\"combat_velocity\":[%ld,%ld,%ld],\"set_speed\":%ld,\"gravity\":[%ld,%ld,%ld],\"keys\":[",
                   model,before[7][0x86],player,scanner,(int)(signed char)before[7][0x14d],ref_pl_frame_tics,(long)ref_pl_altitude,ref_pl_mode,ref_pl_flags,ref_pl_combat,
                   ref_pl_combat_velocity[0],ref_pl_combat_velocity[1],ref_pl_combat_velocity[2],(long)ref_pl_set_speed,
                   (long)(int)gravity_before[0],(long)(int)gravity_before[1],(long)(int)gravity_before[2]);
            for(i=0;i<10;++i) printf("%s%d",i?",":"",pressed[i]);
            printf("],\"bodies\":[");
            for(k=1;k<8;++k) {
                printf("%s{\"parent\":%u,\"kind\":%u,\"radius\":%d,\"mass\":[%d,%d],\"stamp\":%u,\"position\":[%lld,%lld,%lld],\"velocity\":[%ld,%ld,%ld],\"cache\":[%lld,%lld,%lld],\"after\":{\"parent\":%u,\"stamp\":%u,\"position\":[%lld,%lld,%lld],\"velocity\":[%ld,%ld,%ld],\"cache\":[%lld,%lld,%lld]}}",
                    k>1?",":"",before[k][0x56],before[k][0x87],(int)*(short *)(before[k]+0xda),(int)*(short *)(before[k]+0xa4),(int)*(short *)(before[k]+0xa6),before[k][0x25],
                    get64(before[k],0x3e),get64(before[k],0x46),get64(before[k],0x4e),
                    get32(before[k],0x8c),get32(before[k],0x90),get32(before[k],0x94),
                    get64(before[k],0x26),get64(before[k],0x2e),get64(before[k],0x36),
                    o[k][0x56],o[k][0x25],get64(o[k],0x3e),get64(o[k],0x46),get64(o[k],0x4e),
                    get32(o[k],0x8c),get32(o[k],0x90),get32(o[k],0x94),
                    get64(o[k],0x26),get64(o[k],0x2e),get64(o[k],0x36));
            }
            printf("],\"after\":{\"matrix\":[");
            for(i=0;i<9;++i) printf("%s%ld",i?",":"",get32(object,4*i));
            printf("],\"drift\":%d,\"rate\":[",(int)(signed char)object[0x24]);
            for(i=0;i<3;++i) printf("%s%d",i?",":"",*(short *)(object+0xb0+2*i));
            printf("],\"thrust\":[");
            for(i=0;i<3;++i) printf("%s%d",i?",":"",*(short *)(object+0xb6+2*i));
            printf("],\"cap\":[");
            for(i=0;i<3;++i) printf("%s%ld",i?",":"",get32(object,0xf2+4*i));
            printf("],\"fraction\":[");
            for(i=0;i<3;++i) printf("%s%u",i?",":"",*(unsigned short *)(object+0xec+2*i));
            printf("],\"counter\":%d,\"scanner\":%u,\"autopilot\":%d,\"gravity\":[%ld,%ld,%ld]}}\n",(int)(signed char)object[0x14d],ref_scanner,(int)*(short *)(object+0x112),(long)(int)ref_gravity_x,(long)(int)ref_gravity_y,(long)(int)ref_gravity_z);
        }
    }
    if (fflush(stdout)) fail("cannot flush world-step vectors");
    return 0;
}

/* Collision distance. collide drives the original F569/F570 (the player's
   distance to the body it orbits, the frame switch F572/F573 as it enters and
   leaves the body's influence radius, and the scan of the other physical bodies
   in its frame; F575 and F577 per pair) on a synthetic seven-body tree with the
   player last, random radii, rotating-frame bytes, orientations and a stale
   DATA_008704. Positions keep every pair clear of an actual collision and no
   body is complex; those paths are not ported. */
static void planet_parent(unsigned char *object);
static int has_shape(unsigned long model);
extern int ref_collide(unsigned char *object,unsigned char *list,unsigned char *flags,unsigned char *parent,unsigned char **parent_pointer,unsigned long *distance,unsigned char *pad);
extern unsigned char ref_coll_flags;
extern unsigned long ref_coll_distance, ref_coll_distance_high;
static long long get64(const unsigned char *object,unsigned long offset);
static void print_collision_body(const unsigned char *b,unsigned physical)
{
    unsigned long i;
    printf("{\"kind\":%u,\"radius\":%d,\"mass\":[%d,%d],\"parent\":%u,\"rotating\":%u,\"updated\":%u,\"hidden\":%u,\"model\":%lu,\"outer\":%lld,\"inner\":%lld,\"shape\":%u,\"delay\":%ld,\"physical\":%u,\"stamp\":%u,\"position\":[%lld,%lld,%lld],\"velocity\":[%ld,%ld,%ld],\"cache\":[%lld,%lld,%lld],\"orientation\":[",
        b[0x87],(int)*(short *)(b+0xda),(int)*(short *)(b+0xa4),(int)*(short *)(b+0xa6),b[0x56],b[0x57],b[0x58],b[0x8b],(unsigned long)get32(b,0x82)&0xffffffffUL,get64(b,0x138),get64(b,0x140),b[0x14c],get32(b,0x148),physical,b[0x25],
        get64(b,0x3e),get64(b,0x46),get64(b,0x4e),get32(b,0x8c),get32(b,0x90),get32(b,0x94),get64(b,0x26),get64(b,0x2e),get64(b,0x36));
    for(i=0;i<9;++i) printf("%s%ld",i?",":"",get32(b,4*i));
    printf("],\"seed\":%lu,\"words\":[",(unsigned long)get32(b,0xa0)&0xffffffffUL);
    for(i=0;i<91;++i) printf("%s%u",i?",":"",(unsigned)*(const unsigned short *)(b+0x9c+2*i));
    printf("]}");
}
static int collide_vectors(void)
{
    static unsigned char list[0x20000];
    unsigned char *o[8];
    unsigned char before[8][0x152],physical_before[8];
    unsigned char flags=0,parent_index=0,pad=0,*parent_pointer=0;
    unsigned long n,i,k,distance=0,high_before;
    int result;
    fprintf(stderr,"JJFFE collide-v1: original F569/F570 (distance to the parent body, the F572/F573 switch between linear and rotating frames, the scan of the other physical bodies; F575, F577 per pair) on a synthetic seven-body tree with the player last, random outer radii, rotating bytes, orientations and a stale DATA_008704; positions keep every pair clear of a collision and no body is complex; the bodies before and after, the return value, flags, distance and the distance registers.\n");
    texture_rng=0x6d2b79f5UL;
    for(n=0;n<4096;++n) {
        memset(list,0,0x74+0x152*8+0x100);
        for(k=0;k<8;++k) o[k]=list+0x74+0x152*k;
        ref_pl_object_list=list;
        ref_list_pointer=list;
        ref_frame_stamp=(unsigned char)texture_random(0,3);
        ref_pl_frame_tics=(unsigned long)texture_random(0,(n&1)?0x80000:200);
        for(k=1;k<8;++k) {
            long long position,outer,inner;
            unsigned int scale;
            o[k][0x56]=(unsigned char)(k==1?0:texture_random(0,(long)k-1));
            o[k][0x86]=(unsigned char)k;
            o[k][0x87]=(unsigned char)texture_random(2,9);
            o[k][0x57]=(unsigned char)(texture_random(0,2)==0?1:0);
            o[k][0x58]=(unsigned char)texture_random(0,255);
            o[k][0x8b]=(unsigned char)(texture_random(0,9)==0?1:0);
            { unsigned long model=(unsigned long)(texture_random(0,9)==0?0x99:texture_random(0,0x80)); memcpy(o[k]+0x82,&model,4); }
            o[k][0x14c]=(unsigned char)texture_random(0,0x7f);
            { long delay=(texture_random(0,3)==0)?texture_random(1,300000):0; memcpy(o[k]+0x148,&delay,4); }
            scale=(unsigned int)texture_random(8,48);
            outer=(long long)((((unsigned long long)(unsigned long)texture_random(0,0x7fffffff)<<32)|(unsigned long long)(unsigned long)texture_random(0,0x7fffffff))>>scale);
            memcpy(o[k]+0x138,&outer,8);
            inner=(long long)texture_random(1,0x400);
            if(texture_random(0,2)==0) {outer=inner;memcpy(o[k]+0x138,&outer,8);}
            memcpy(o[k]+0x140,&inner,8);
            o[k][0x25]=(unsigned char)texture_random(0,3);
            list[k]=(unsigned char)((texture_random(0,3)?0x10:0)|(texture_random(0,5)?0x4:0));
            for(i=0;i<3;++i) {
                scale=(unsigned int)texture_random(0,36);
                position=(long long)((((unsigned long long)(unsigned long)texture_random(0,0x7fffffff)<<32)|(unsigned long long)(unsigned long)texture_random(0,0x7fffffff))>>scale)+0x1000000LL;
                if(texture_random(0,1)) position=-position;
                memcpy(o[k] + 0x3e + 8*i,&position,8);
                position=(long long)((((unsigned long long)(unsigned long)texture_random(0,0x7fffffff)<<32)|(unsigned long long)(unsigned long)texture_random(0,0x7fffffff))>>scale);
                memcpy(o[k] + 0x26 + 8*i,&position,8);
                put32(o[k],0x8c+4*i,(long)texture_random(-100000,100000));
            }
            for(i=0;i<9;++i) put32(o[k],4*i,(long)(((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff)));
        }
        o[7][0x8b]=0; o[7][0x57]=(unsigned char)texture_random(0,1);
        if(o[7][0x56] && has_shape((unsigned long)get32(o[7],0x82)) && texture_random(0,1)) planet_parent(o[o[7][0x56]]);
        /* Aim the player (and sibling 6) into the band where the pair is in
           reach but not in contact: distance 0.4 to 2.5 of the radius sum. */
        {
            unsigned int target;
            for(target=0;target<2;++target) {
                unsigned long ka=target?6:7;
                unsigned long kb=target?7:o[7][0x56];
                long long radius_a,radius_b,position;
                double sum,length,scale,component[3];
                if(kb==0) continue;
                if(target) {o[6][0x56]=o[7][0x56];o[6][0x57]=o[7][0x57];if(o[6][0x56]==6) o[6][0x56]=0;}
                memcpy(&radius_a,o[ka]+0x138,8);memcpy(&radius_b,o[kb]+0x138,8);
                sum=(double)radius_a+(double)radius_b;
                for(i=0;i<3;++i) component[i]=(double)texture_random(1,0x7fffffff)*(texture_random(0,1)?1.0:-1.0);
                length=component[0]*component[0]+component[1]*component[1]+component[2]*component[2];
                length=sqrt(length);
                scale=sum*(double)texture_random(400,2500)/1000.0/length;
                for(i=0;i<3;++i) {
                    position=(long long)(component[i]*scale);
                    if(target) {long long base; memcpy(&base,o[7] + 0x3e + 8*i,8); position=base-position;}
                    memcpy(o[ka] + 0x3e + 8*i,&position,8);
                }
            }
        }
        high_before=(unsigned long)(texture_random(0,3)==0?texture_random(-1,1):0);
        ref_coll_distance_high=high_before;
        ref_coll_distance=0x12345678UL;
        for(k=0;k<8;++k) {memcpy(before[k],o[k],0x152);physical_before[k]=(unsigned char)(list[k]&4?1:0);}
        result=ref_collide(o[7],list,&flags,&parent_index,&parent_pointer,&distance,&pad);
        printf("{\"case\":%lu,\"stamp\":%u,\"tics\":%lu,\"high\":%ld,\"bodies\":[",n,(unsigned)ref_frame_stamp,ref_pl_frame_tics,(long)(int)high_before);
        for(k=1;k<8;++k) {printf("%s",k>1?",":"");printf("{\"before\":");print_collision_body(before[k],physical_before[k]);printf(",\"after\":");print_collision_body(o[k],list[k]&4?1u:0u);printf("}");}
        printf("],\"result\":%d,\"flags\":%u,\"distance\":%ld,\"object\":%u,\"pad\":%u,\"register\":[%lu,%lu]}\n",result&0xff,(unsigned)flags,(long)(int)distance,(unsigned)parent_index,(unsigned)pad,ref_coll_distance&0xffffffffUL,ref_coll_distance_high&0xffffffffUL);
    }
    if (fflush(stdout)) fail("cannot flush collide vectors");
    return 0;
}

/* World tick. world-tick is world-step plus the collision scan the player's
   tick ends with: see the banner printed below. */
static int world_tick_vectors(void)
{
    static unsigned char list[0x20000];
    unsigned char *o[8];
    unsigned char *object;
    unsigned char before[8][0x152];
    unsigned long n,i,k,model,time_low,time_high,gravity_before[3],distance=0,high_before;
    unsigned char coll_flags=0,parent_index=0,pad=0,*parent_pointer=0,physical_before[8],live_before[8];
    int collision_result=0;
    long long time64;
    static const int key_offsets[10]={0x1c,0x36,0x48,0x4c,0x4b,0x4d,0x4f,0x51,0x49,0x47};
    int pressed[10];
    fprintf(stderr,"JJFFE world-tick-v1: one whole player tick of the original in a world (F1041 with F585 twice, F603 gravity, F609 auto-roll, the F606 countdown and the thrust request, then F601/F602, F1512 and the collision scan F569/F570 with the F572/F573 frame switch) for the player as the last body of a synthetic seven-body tree with ffp masses, radii, rotating bytes, orientations and a stale DATA_008704; the bodies before and after, the player's matrix, rates, thrust, caps, fractions, countdown, scanner, auto-roll rate, gravity globals, flags, distance and registers.\n");
    texture_rng=0x2f6b1d93UL;
    for(n=0;n<4096;++n) {
        memset(list,0,0x74+0x152*8+0x100);
        for(k=0;k<8;++k) o[k]=list+0x74+0x152*k;
        object=o[7];
        ref_pl_object_list=list;
        ref_list_pointer=list;
        ref_frame_stamp=(unsigned char)texture_random(0,3);
        for(k=1;k<8;++k) {
            long long position,outer,inner;
            unsigned int scale;
            o[k][0x56]=(unsigned char)(k==1?0:texture_random(0,(long)k-1));
            o[k][0x86]=(unsigned char)k;
            o[k][0x87]=(unsigned char)texture_random(2,9);
            { short mantissa=(texture_random(0,9)==0)?0:(short)texture_random(0x4000,0x7fff),exponent=(short)texture_random(-8,70),frame_radius=(short)texture_random(0,40);
              memcpy(o[k]+0xa4,&mantissa,2);memcpy(o[k]+0xa6,&exponent,2);memcpy(o[k]+0xda,&frame_radius,2); }
            o[k][0x57]=(unsigned char)(texture_random(0,2)==0?1:0);
            o[k][0x58]=(unsigned char)texture_random(0,255);
            o[k][0x8b]=(unsigned char)(texture_random(0,9)==0?1:0);
            { unsigned long model=(unsigned long)(texture_random(0,9)==0?0x99:texture_random(0,0x80)); memcpy(o[k]+0x82,&model,4); }
            o[k][0x14c]=(unsigned char)texture_random(0,0x7f);
            { long delay=(texture_random(0,3)==0)?texture_random(1,300000):0; memcpy(o[k]+0x148,&delay,4); }
            scale=(unsigned int)texture_random(8,48);
            outer=(long long)((((unsigned long long)(unsigned long)texture_random(0,0x7fffffff)<<32)|(unsigned long long)(unsigned long)texture_random(0,0x7fffffff))>>scale);
            memcpy(o[k]+0x138,&outer,8);
            inner=(long long)texture_random(1,0x400);
            if(texture_random(0,2)==0) {outer=inner;memcpy(o[k]+0x138,&outer,8);}
            memcpy(o[k]+0x140,&inner,8);
            o[k][0x25]=(unsigned char)texture_random(0,3);
            list[k]=(unsigned char)((texture_random(0,3)?0x10:0)|(texture_random(0,5)?0x4:0));
            for(i=0;i<3;++i) {
                scale=(unsigned int)texture_random(0,36);
                position=(long long)((((unsigned long long)(unsigned long)texture_random(0,0x7fffffff)<<32)|(unsigned long long)(unsigned long)texture_random(0,0x7fffffff))>>scale)+0x1000000LL;
                if(texture_random(0,1)) position=-position;
                memcpy(o[k] + 0x3e + 8*i,&position,8);
                position=(long long)((((unsigned long long)(unsigned long)texture_random(0,0x7fffffff)<<32)|(unsigned long long)(unsigned long)texture_random(0,0x7fffffff))>>scale);
                memcpy(o[k] + 0x26 + 8*i,&position,8);
                put32(o[k],0x8c+4*i,(long)texture_random(-100000,100000));
            }
            for(i=0;i<9;++i) put32(o[k],4*i,(long)(((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff)));
        }
        o[7][0x8b]=0; o[7][0x57]=(unsigned char)texture_random(0,1);
        if(o[7][0x56] && has_shape((unsigned long)get32(o[7],0x82)) && texture_random(0,1)) planet_parent(o[o[7][0x56]]);
        /* Aim the player (and sibling 6) into the band where the pair is in
           reach but not in contact: distance 0.4 to 2.5 of the radius sum. */
        {
            unsigned int target;
            for(target=0;target<2;++target) {
                unsigned long ka=target?6:7;
                unsigned long kb=target?7:o[7][0x56];
                long long radius_a,radius_b,position;
                double sum,length,scale,component[3];
                if(kb==0) continue;
                if(target) {o[6][0x56]=o[7][0x56];o[6][0x57]=o[7][0x57];if(o[6][0x56]==6) o[6][0x56]=0;}
                memcpy(&radius_a,o[ka]+0x138,8);memcpy(&radius_b,o[kb]+0x138,8);
                sum=(double)radius_a+(double)radius_b;
                for(i=0;i<3;++i) component[i]=(double)texture_random(1,0x7fffffff)*(texture_random(0,1)?1.0:-1.0);
                length=component[0]*component[0]+component[1]*component[1]+component[2]*component[2];
                length=sqrt(length);
                scale=sum*(double)texture_random(400,2500)/1000.0/length;
                for(i=0;i<3;++i) {
                    position=(long long)(component[i]*scale);
                    if(target) {long long base; memcpy(&base,o[7] + 0x3e + 8*i,8); position=base-position;}
                    memcpy(o[ka] + 0x3e + 8*i,&position,8);
                }
            }
        }
        if(n&1) {
            for(i=0;i<3;++i) put32(object,12*i+4*i,0x7fffffffL-texture_random(0,100000));
            for(i=0;i<9;++i) if(i%4) put32(object,4*i,(long)texture_random(-3000000,3000000));
        } else for(i=0;i<9;++i) put32(object,4*i,(long)(((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff)));
        object[0x24]=(unsigned char)(signed char)texture_random(-2,0x40);
        for(i=0;i<3;++i) {
            short rate,lim;
            switch(texture_random(0,3)) {
            case 0: rate=0; break;
            case 1: rate=(short)texture_random(-14,14); break;
            default: rate=(short)texture_random(-4000,4000); break;
            }
            memcpy(object+0xb0+2*i,&rate,2);
            lim=(short)texture_random(0,(n&16)?32767:6000); memcpy(object + 0xbc + 4*i,&lim,2);
            lim=(short)-texture_random(0,(n&32)?32767:6000); memcpy(object + 0xbe + 4*i,&lim,2);
            ref_pl_combat_velocity[i]=(long)texture_random(-3000000,3000000);
        }
        model=(unsigned long)texture_random(14,70);
        memcpy(object+0x82,&model,4);
        object[0x86]=7;
        object[0x14d]=(unsigned char)(signed char)texture_random(-1,0xd);
        ref_player_craft=(unsigned long)texture_random(5,7);
        ref_scanner=(unsigned short)texture_random(0,0xffff);
        ref_pl_frame_tics=(unsigned long)texture_random(0,(n&64)?0x80000:0x4000);
        ref_pl_altitude=(unsigned long)(texture_random(0,1)?texture_random(-1000,0x2625a00):0x2625a01L+texture_random(0,0x7fffffff-0x2625a01));
        ref_pl_mode=(unsigned char)(texture_random(0,5)==0?0x24:0);
        ref_pl_flags=(unsigned char)texture_random(0,1);
        ref_pl_combat=(unsigned char)(texture_random(0,1)?2:0);
        switch(texture_random(0,3)) {
        case 0: ref_pl_set_speed=0; break;
        case 1: ref_pl_set_speed=(unsigned long)texture_random(-5000,5000); break;
        case 2: ref_pl_set_speed=(unsigned long)texture_random(-3000000,3000000); break;
        default: ref_pl_set_speed=(unsigned long)texture_random(-100000000,100000000); break;
        }
        memset(ref_pl_keys,0x80,sizeof(ref_pl_keys));
        for(i=0;i<10;++i) {pressed[i]=texture_random(0,3)==0;if(pressed[i]) ref_pl_keys[key_offsets[i]]=0;}
        switch(texture_random(0,3)) {
        case 0: time64=(long long)texture_random(0,2000); break;
        case 1: time64=(long long)texture_random(0,100000); break;
        case 2: time64=(long long)texture_random(0,0x7fffffff); break;
        default: time64=((long long)texture_random(0,0xff)<<32)|(long long)texture_random(0,0x7fffffff); break;
        }
        time_low=(unsigned long)(time64&0xffffffffLL);time_high=(unsigned long)(time64>>32);
        gravity_before[0]=ref_gravity_x=(unsigned long)texture_random(-2000,2000);
        gravity_before[1]=ref_gravity_y=(unsigned long)texture_random(-2000,2000);
        gravity_before[2]=ref_gravity_z=(unsigned long)texture_random(-2000,2000);
        high_before=(unsigned long)(texture_random(0,3)==0?texture_random(-1,1):0);
        ref_coll_distance_high=high_before;
        ref_coll_distance=0x12345678UL;
        for(k=0;k<8;++k) {memcpy(before[k],o[k],0x152);physical_before[k]=(unsigned char)(list[k]&4?1:0);live_before[k]=(unsigned char)(list[k]&0x10?1:0);}
        {
            unsigned long player=ref_player_craft; unsigned short scanner=ref_scanner;
            ref_player_orient(object);
            if(ref_pl_flags&1) ref_accelerate_uncapped(object,time_low); else ref_accelerate(object,time_low);
            ref_advance(object,time_high,time_low);
            ref_pl_frame_tics=ref_pl_frame_tics;
            collision_result=ref_collide(object,list,&coll_flags,&parent_index,&parent_pointer,&distance,&pad);
            printf("{\"case\":%lu,\"time\":%lld,\"stamp\":%u,\"high\":%ld,\"live\":[",n,time64,(unsigned)ref_frame_stamp,(long)(int)high_before);
            for(k=1;k<8;++k) printf("%s%u",k>1?",":"",(unsigned)live_before[k]);
            printf("],\"matrix\":[");
            for(i=0;i<9;++i) printf("%s%ld",i?",":"",get32(before[7],4*i));
            printf("],\"drift\":%d,\"rate\":[",(int)(signed char)before[7][0x24]);
            for(i=0;i<3;++i) printf("%s%d",i?",":"",*(short *)(before[7]+0xb0+2*i));
            printf("],\"limits\":[");
            for(i=0;i<3;++i) printf("%s%d,%d",i?",":"",*(short *)(before[7] + 0xbc + 4*i),*(short *)(before[7] + 0xbe + 4*i));
            printf("],\"model\":%lu,\"craft\":%u,\"player\":%lu,\"scanner\":%u,\"counter\":%d,\"tics\":%lu,\"altitude\":%ld,\"mode\":%u,\"flags\":%u,\"combat\":%u,\"combat_velocity\":[%ld,%ld,%ld],\"set_speed\":%ld,\"gravity\":[%ld,%ld,%ld],\"keys\":[",
                   model,before[7][0x86],player,scanner,(int)(signed char)before[7][0x14d],ref_pl_frame_tics,(long)ref_pl_altitude,ref_pl_mode,ref_pl_flags,ref_pl_combat,
                   ref_pl_combat_velocity[0],ref_pl_combat_velocity[1],ref_pl_combat_velocity[2],(long)ref_pl_set_speed,
                   (long)(int)gravity_before[0],(long)(int)gravity_before[1],(long)(int)gravity_before[2]);
            for(i=0;i<10;++i) printf("%s%d",i?",":"",pressed[i]);
            printf("],\"bodies\":[");
            for(k=1;k<8;++k) {printf("%s",k>1?",":"");printf("{\"before\":");print_collision_body(before[k],physical_before[k]);printf(",\"after\":");print_collision_body(o[k],list[k]&4?1u:0u);printf("}");}
            printf("],\"after\":{\"matrix\":[");
            for(i=0;i<9;++i) printf("%s%ld",i?",":"",get32(object,4*i));
            printf("],\"drift\":%d,\"rate\":[",(int)(signed char)object[0x24]);
            for(i=0;i<3;++i) printf("%s%d",i?",":"",*(short *)(object+0xb0+2*i));
            printf("],\"thrust\":[");
            for(i=0;i<3;++i) printf("%s%d",i?",":"",*(short *)(object+0xb6+2*i));
            printf("],\"cap\":[");
            for(i=0;i<3;++i) printf("%s%ld",i?",":"",get32(object,0xf2+4*i));
            printf("],\"fraction\":[");
            for(i=0;i<3;++i) printf("%s%u",i?",":"",*(unsigned short *)(object+0xec+2*i));
            printf("],\"counter\":%d,\"scanner\":%u,\"autopilot\":%d,\"gravity\":[%ld,%ld,%ld]},\"contact\":{\"result\":%d,\"flags\":%u,\"distance\":%ld,\"object\":%u,\"pad\":%u,\"register\":[%lu,%lu]}}\n",(int)(signed char)object[0x14d],ref_scanner,(int)*(short *)(object+0x112),(long)(int)ref_gravity_x,(long)(int)ref_gravity_y,(long)(int)ref_gravity_z,collision_result&0xff,(unsigned)coll_flags,(long)(int)distance,(unsigned)parent_index,(unsigned)pad,ref_coll_distance&0xffffffffUL,ref_coll_distance_high&0xffffffffUL);
        }
    }
    if (fflush(stdout)) fail("cannot flush world-tick vectors");
    return 0;
}

/* Galaxy table lookups. galaxy drives the original F868 (system index of a
   code: the table, four special codes, or a class by distance from the
   centre), and for table systems F869 (details), F870 (market parameters and
   the stock string left in DATA_008790) and F871 (control and ranks, with the
   player's two rank globals set per case), over every table code, the special
   codes and random codes near and far. */
extern int ref_galaxy_index(unsigned long code);
extern void ref_galaxy_details(unsigned long code,unsigned long *a,unsigned long *b,unsigned long *c,unsigned long *d,unsigned long *e,unsigned long *f,unsigned long *g,unsigned long *h);
extern void ref_galaxy_market(unsigned long code,unsigned char **stock,unsigned long *population,unsigned long *danger,unsigned long *allegiance);
extern void ref_galaxy_control(unsigned long code,unsigned long *federal,unsigned long *imperial,unsigned long *maximum,unsigned long *allegiance,unsigned long *minor,unsigned long *major);
extern unsigned char ref_galaxy_stock[40];
extern unsigned long ref_galaxy_rank_a,ref_galaxy_rank_b;
static int galaxy_vectors(void)
{
    static unsigned long codes[8192];
    unsigned long n,count=0,i,x,y;
    extern unsigned char ref_galaxy_table[];
    extern unsigned long ref_galaxy_special[4];
    fprintf(stderr,"JJFFE galaxy-v1: original F868 (system index or class), and for table systems F869, F870 (stock string in DATA_008790) and F871 (player ranks DATA_008912/DATA_008913 set per case) over every table code, the four special codes and random codes near and far from the galactic centre.\n");
    {
        unsigned char *table=ref_galaxy_table;
        for(i=0;i<330;++i) {unsigned long c;memcpy(&c,table+54*i,4);if(c==0)break;codes[count++]=c;}
    }
    texture_rng=0x4e67c6a7UL;
    for(i=0;i<4;++i) {unsigned long c=ref_galaxy_special[i];codes[count++]=c;codes[count++]=c|((unsigned long)texture_random(1,63)<<26);}
    for(n=0;n<2000;++n) codes[count++]=((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff);
    for(n=0;n<2000;++n) {
        x=0x1718+(unsigned long)texture_random(-150,150);y=0x1524+(unsigned long)texture_random(-150,150);
        codes[count++]=(x&0x1fff)|((y&0x1fff)<<13)|((unsigned long)texture_random(0,63)<<26);
    }
    for(n=0;n<1500;++n) {
        x=0x1718+(unsigned long)texture_random(-12,12);y=0x1524+(unsigned long)texture_random(-12,12);
        codes[count++]=(x&0x1fff)|((y&0x1fff)<<13)|((unsigned long)texture_random(0,63)<<26);
    }
    for(n=0;n<count;++n) {
        unsigned long code=codes[n],a[9],stock_ptr_value;
        unsigned char *stock;
        unsigned long population,danger,allegiance,federal,imperial,maximum,control_allegiance,minor,major;
        int index=ref_galaxy_index(code);
        printf("{\"code\":%lu,\"index\":%d",code,index);
        if(index>=0 || index<0) {
            ref_galaxy_rank_a=(unsigned long)texture_random(0,100);ref_galaxy_rank_b=(unsigned long)texture_random(0,100);
            ref_galaxy_details(code,&a[0],&a[1],&a[2],&a[3],&a[4],&a[5],&a[6],&a[7]);
            printf(",\"details\":[%lu,%lu,%lu,%lu,%lu,%lu,%lu,%lu]",a[0],a[1],a[2],a[3],a[4],a[5],a[6],a[7]);
            memset(ref_galaxy_stock,0x55,40);
            ref_galaxy_market(code,&stock,&population,&danger,&allegiance);
            stock_ptr_value=(unsigned long)stock;(void)stock_ptr_value;
            printf(",\"market\":{\"population\":%lu,\"danger\":%lu,\"allegiance\":%lu,\"stock\":[",population,danger,allegiance);
            for(i=0;i<37;++i) printf("%s%u",i?",":"",ref_galaxy_stock[i]);
            printf("]}");
            ref_galaxy_control(code,&federal,&imperial,&maximum,&control_allegiance,&minor,&major);
            printf(",\"control\":{\"ranks\":[%lu,%lu],\"federal\":%lu,\"imperial\":%lu,\"maximum\":%lu,\"allegiance\":%lu,\"minor\":%lu,\"major\":%lu}",ref_galaxy_rank_a,ref_galaxy_rank_b,federal,imperial,maximum,control_allegiance,minor,major);
        }
        printf("}\n");
    }
    if (fflush(stdout)) fail("cannot flush galaxy vectors");
    return 0;
}

/* System generation. sysgen drives the original F878 on a zeroed object list
   for synthetic parameters (seeds, total mass, star count, system code and the
   population-like DATA_008854), after building the hand-coded objects with
   F875 (the templates F891 and F892 copy), and prints the generated objects
   (names zeroed only for hand-coded systems, whose names come from the executable's strings), the counters and the
   generator's random state. A separate record gives the hand-coded array. */
extern void ref_sysgen(unsigned char *list,unsigned char *params);
extern void ref_handcoded_init(void);
extern unsigned long ref_sg_index_table[];
extern unsigned char ref_handcoded_base[];
extern unsigned long ref_sg_a,ref_sg_b,ref_sg_seed_a,ref_sg_seed_b,ref_sg_government,ref_sg_population;
extern unsigned char ref_sg_ports,ref_sg_stations;
/* F877 keeps counters in uninitialised locals: clear the stack below so the names built from them are reproducible between runs. */
static __attribute__((noinline)) void scrub_stack(void)
{
    volatile unsigned char scrap[32768];
    unsigned long i;
    for(i=0;i<sizeof(scrap);++i) scrap[i]=0;
}
static void sysgen_object(const unsigned char *o,int mask)
{
    unsigned char copy[68];
    unsigned long i;
    memcpy(copy,o,68);
    if(mask) memset(copy+0x26,0,20);
    printf("[");
    for(i=0;i<68;++i) printf("%s%u",i?",":"",copy[i]);
    printf("]");
}
static sigjmp_buf sysgen_jump;
static void sysgen_alarm(int signal_number) {(void)signal_number;siglongjmp(sysgen_jump,1);}
static int sysgen_vectors(void)
{
    static unsigned char list[0x50*68+0x100];
    unsigned char params[0x40];
    unsigned long n,i,count;
    fprintf(stderr,"JJFFE sysgen-v1: original F878 for synthetic seeds, total mass, star counts, system codes and populations, after F875 builds the hand-coded objects; the hand-coded array first, then the generated objects , station counters and the random state.\n");
    texture_rng=0x1b873593UL;
    ref_sg_seed_a=0x1234;ref_sg_seed_b=0x5678;
    ref_handcoded_init();
    printf("{\"handcoded\":[");
    for(i=0;i<146;++i) {printf("%s",i?",":"");sysgen_object(ref_handcoded_base+68*i,0);}
    printf("],\"seeds\":[%lu,%lu]}\n",ref_sg_seed_a,ref_sg_seed_b);
    for(n=0;n<4160;++n) {
        unsigned long seed,mass,stars,code,population,shift;
        memset(list,0,sizeof(list));
        memset(params,0,sizeof(params));
        seed=((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(1,0xffff);
        shift=(unsigned long)texture_random(0,8);
        mass=0x1036640UL+((((unsigned long)texture_random(0,0x7fff)<<17)|(unsigned long)texture_random(0,0x1ffff))>>shift);
        switch(texture_random(0,7)) {
        case 0: case 1: case 2: case 3: stars=0; break;
        case 4: case 5: stars=1; break;
        default: stars=(unsigned long)texture_random(2,7); break;
        }
        code=((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff);
        population=(unsigned long)texture_random(0,(n&1)?255:60);
        if(n>=4096) {
            /* Hand-coded systems: a zero first seed; the codes the index table knows, then others. */
            seed=0;
            code=(n-4096<12)?ref_sg_index_table[2*(n-4096)]:code;
        }
        memcpy(params,&seed,4);memcpy(params+4,&mass,4);memcpy(params+8,&stars,4);memcpy(params+0xc,&code,4);memcpy(params+0x10,&population,4);
        strcpy((char *)params+0x14,"Testname");
        ref_sg_population=population;
        if(getenv("SYSGEN_DEBUG")) {fprintf(stderr,"case %lu seed %lu mass %lu stars %lu code %lu pop %lu\n",n,seed,mass,stars,code,population);fflush(stderr);}
        {
            struct itimerval timer;
            int hung;
            memset(&timer,0,sizeof(timer));
            signal(SIGALRM,sysgen_alarm);
            hung=sigsetjmp(sysgen_jump,1);
            if(!hung) {
                timer.it_value.tv_usec=300000;
                setitimer(ITIMER_REAL,&timer,0);
                scrub_stack();
                ref_sysgen(list,params);
            }
            memset(&timer,0,sizeof(timer));
            setitimer(ITIMER_REAL,&timer,0);
            if(hung) {
                printf("{\"case\":%lu,\"seed\":%lu,\"mass\":%lu,\"stars\":%lu,\"code\":%lu,\"population\":%lu,\"hung\":true}\n",n,seed,mass,stars,code,population);
                continue;
            }
        }
        for(count=0;count<0x50 && memcmp(list+68*count,"\0\0\0\0",4)!=0;++count) ;
        printf("{\"case\":%lu,\"seed\":%lu,\"mass\":%lu,\"stars\":%lu,\"code\":%lu,\"population\":%lu,\"count\":%lu,\"ports\":%u,\"stations\":%u,\"state\":[%lu,%lu],\"allegiance\":%lu,\"objects\":[",n,seed,mass,stars,code,population,count,ref_sg_ports,ref_sg_stations,ref_sg_a&0xffffffffUL,ref_sg_b&0xffffffffUL,ref_sg_government&0xffffffffUL);
        for(i=0;i<=count && i<0x50;++i) {printf("%s",i?",":"");sysgen_object(list+68*i,0);}
        printf("]}\n");
    }
    if (fflush(stdout)) fail("cannot flush sysgen vectors");
    return 0;
}

/* World objects. worldgen drives the original F880 (and through it F927,
   F1533, F879 and the orbit helpers) on the objects F878 generates for a
   synthetic system, at a synthetic clock, into a zeroed physics list; it prints
   the generated system objects (names zeroed, +0x18 as a physics index) and every
   flagged physics object . */
extern void ref_worldgen(unsigned char *list,unsigned char *system,unsigned long low,unsigned long high);
extern long ref_worldgen_counter;
extern unsigned char ref_player_struct[0x4000];
static int worldgen_vectors(void)
{
    static unsigned char system[0x50*68+0x100];
    static unsigned char physics[0x74+0x152*0x73+0x100];
    unsigned char params[0x40];
    unsigned long n,i,count,low,high;
    fprintf(stderr,"JJFFE worldgen-v1: original F880 (F927, F1533, F879, F1506-F1510, F1674, F1675) for the objects F878 generates for synthetic systems, at synthetic clocks, into a zeroed physics list; the system objects (names zeroed, +0x18 as an index) and every flagged physics object .\n");
    texture_rng=0x2c1b3c6dUL;
    ref_sg_seed_a=0x1234;ref_sg_seed_b=0x5678;
    ref_handcoded_init();
    for(n=0;n<1024;++n) {
        unsigned long seed,mass,stars,code,population,shift,k;
        struct itimerval timer;
        int hung;
        memset(system,0,sizeof(system));
        memset(physics,0,sizeof(physics));
        memset(params,0,sizeof(params));
        seed=((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(1,0xffff);
        shift=(unsigned long)texture_random(0,8);
        mass=0x1036640UL+((((unsigned long)texture_random(0,0x7fff)<<17)|(unsigned long)texture_random(0,0x1ffff))>>shift);
        stars=(unsigned long)(texture_random(0,3)==0?texture_random(1,3):0);
        code=((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff);
        population=(unsigned long)texture_random(0,255);
        if(n>=1000) {seed=0;code=ref_sg_index_table[2*(n-1000)%24];}
        memcpy(params,&seed,4);memcpy(params+4,&mass,4);memcpy(params+8,&stars,4);memcpy(params+0xc,&code,4);memcpy(params+0x10,&population,4);
        strcpy((char *)params+0x14,"Testname");
        ref_sg_population=population;
        low=((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff);
        high=(unsigned long)texture_random(0,3000);
        memcpy(ref_player_struct,&low,4);memcpy(ref_player_struct+4,&high,4);
        ref_pl_object_list=physics;ref_list_pointer=physics;
        ref_frame_stamp=(unsigned char)texture_random(0,200);
        ref_worldgen_counter=0;ref_coll_distance_high=0;
        memset(&timer,0,sizeof(timer));
        signal(SIGALRM,sysgen_alarm);
        hung=sigsetjmp(sysgen_jump,1);
        if(!hung) {
            timer.it_value.tv_usec=500000;
            setitimer(ITIMER_REAL,&timer,0);
            scrub_stack();
            ref_sysgen(system,params);
            ref_worldgen(physics,system,low,high);
        }
        memset(&timer,0,sizeof(timer));
        setitimer(ITIMER_REAL,&timer,0);
        if(hung) {printf("{\"case\":%lu,\"hung\":true}\n",n);continue;}
        for(count=0;count<0x50 && memcmp(system+68*count,"\0\0\0\0",4)!=0;++count) ;
        printf("{\"case\":%lu,\"seed\":%lu,\"mass\":%lu,\"stars\":%lu,\"code\":%lu,\"population\":%lu,\"low\":%lu,\"high\":%lu,\"stamp\":%u,\"count\":%lu,\"counter\":%ld,\"system\":[",n,seed,mass,stars,code,population,low,high,(unsigned)ref_frame_stamp,count,ref_worldgen_counter);
        for(i=0;i<count;++i) {
            unsigned char copy[68];
            unsigned long pointer;
            memcpy(copy,system+68*i,68);
            memcpy(&pointer,copy+0x18,4);
            if(pointer) pointer=(pointer-(unsigned long)physics-0x74)/0x152;
            memcpy(copy+0x18,&pointer,4);
            printf("%s[",i?",":"");
            for(k=0;k<68;++k) printf("%s%u",k?",":"",copy[k]);
            printf("]");
        }
        printf("],\"flags\":[");
        for(i=0;i<0x73;++i) printf("%s%u",i?",":"",physics[i]);
        printf("],\"objects\":{");
        {
            int first=1;
            for(i=0;i<0x73;++i) {
                unsigned char copy[0x152];
                if(!physics[i]) continue;
                memcpy(copy,physics+0x74+0x152*i,0x152);
                printf("%s\"%lu\":[",first?"":",",i);first=0;
                for(k=0;k<0x152;++k) printf("%s%u",k?",":"",copy[k]);
                printf("]");
            }
        }
        printf("}}\n");
    }
    if (fflush(stdout)) fail("cannot flush worldgen vectors");
    return 0;
}

/* Terrain midpoints. terrain-mid drives the original midpoint routines
   F1849-F1867 (chosen through the table at DATA_007844), with F1829, F1846 and
   the distance-mode vertex functions F1831, F1832 and F1848, on synthetic
   triangle-edge nodes from the original arenas (F1816, F1818, F1820): the three
   nodes (0x48 bytes), the edge record and the layer tables (DATA_009260) before
   and after. */
extern void ref_tr_reset(void);
extern void ref_tr_init(void);
extern unsigned char *ref_tr_node(void);
extern unsigned char *ref_tr_edge(void);
extern unsigned long ref_tr_midtab[32];
extern unsigned long ref_tr_fn[5];
extern unsigned long ref_tr_level_base;
extern unsigned long ref_tr_layers[128];
extern unsigned long ref_tr_nodes_used,ref_tr_edges_used;
extern unsigned char ref_tr_map0[16384],ref_tr_map1[16384];
extern unsigned char ref_tr_pool[108000],ref_tr_edge_pool[48000];
extern unsigned char ref_tr_f1831[],ref_tr_f1832[],ref_tr_f1848[];
static unsigned long terrain_word(void)
{
    unsigned long a=(unsigned long)texture_random(0,0xffff),b=(unsigned long)texture_random(0,0xffff);
    switch(texture_random(0,5)) {
        case 0: return a&0x7f;
        case 1: return (a&0x7f)|((b&0x7f)<<16);
        case 2: return (a<<16|b)>>(unsigned)texture_random(0,31);
        default: return (a<<16)|b;
    }
}
static void terrain_node_print(const unsigned char *n)
{
    int k;
    for(k=0;k<0x48;++k) printf("%s%u",k?",":"",n[k]);
}
static int terrain_mid_vectors(void)
{
    unsigned long n;
    fprintf(stderr,"JJFFE terrain-mid-v1: original F1849-F1867 midpoint routines (F1829, F1846, F1831, F1832, F1848) on synthetic nodes from the F1816 arenas, with random layer tables over the two height maps.\n");
    texture_rng=0x51f3a7c5UL;
    for(n=0;n<6144;++n) {
        unsigned char *node[4],*edge,*link;
        unsigned char before[4][0x48];
        unsigned long which=n%19,slot,level,k,j,layer[128];
        unsigned long used_nodes,used_edges;
        int preset=texture_random(0,5)==0;
        memset(ref_tr_pool,0,sizeof(ref_tr_pool));memset(ref_tr_edge_pool,0,sizeof(ref_tr_edge_pool));
        ref_tr_reset();ref_tr_init();
        for(k=0;k<4;++k) {
            node[k]=ref_tr_node();
            for(j=0;j<0x48;j+=4) {
                unsigned long w=terrain_word();
                if(j==0x44) w=(unsigned long)texture_random(0,11);
                if(j==0x3c || j==0x40) w=(unsigned long)((long)terrain_word()>>(unsigned)texture_random(0,12));
                if(j==0xc && texture_random(0,2)==0) w=(unsigned long)texture_random(0,200);
                memcpy(node[k]+j,&w,4);
            }
            if(k==3 && preset) continue;
            memcpy(before[k],node[k],0x48);
        }
        edge=ref_tr_edge();
        {unsigned long first=texture_random(0,1)?(unsigned long)node[0]:(unsigned long)node[1];memcpy(edge,&first,4);}
        if(preset) memcpy(edge+8,&node[3],4);
        memcpy(before[3],node[3],0x48);
        ref_tr_fn[2]=(unsigned long)ref_tr_f1831;ref_tr_fn[3]=(unsigned long)ref_tr_f1832;ref_tr_fn[4]=(unsigned long)ref_tr_f1848;
        ref_tr_level_base=(unsigned long)texture_random(0,14);
        for(k=0;k<128;++k) {
            unsigned long choice=(unsigned long)texture_random(0,3);
            layer[k]=choice==0?0:choice==1?(unsigned long)ref_tr_map0:(unsigned long)ref_tr_map1;
            if(k&1) layer[k]=(unsigned long)(texture_random(0,2)==0?0:(texture_random(0,1)?(unsigned long)ref_tr_map0:(unsigned long)ref_tr_map1));
            if(k<64 && (k&1)==0 && texture_random(0,3)==0) layer[k]=0;
            ref_tr_layers[k]=layer[k];
        }
        level=(unsigned long)texture_random(0,10);
        {
            void (*fn)(void *,void *,void *,void *,unsigned long);
            slot=which==0?0:which<=16?7+which:which==17?24:28;
            memcpy(&fn,&ref_tr_midtab[slot],sizeof(fn));
            fn(edge,node[0],node[1],node[2],level);
        }
        used_nodes=ref_tr_nodes_used;used_edges=ref_tr_edges_used;
        printf("{\"case\":%lu,\"which\":%lu,\"level\":%lu,\"base\":%lu,\"preset\":%d,\"pool\":%lu,\"layers\":[",n,which,level,ref_tr_level_base,preset,(unsigned long)ref_tr_pool);
        for(k=0;k<128;++k) printf("%s%d",k?",":"",layer[k]==0?0:layer[k]==(unsigned long)ref_tr_map0?1:2);
        printf("],\"before\":[");
        for(k=0;k<4;++k) {printf("%s[",k?",":"");terrain_node_print(before[k]);printf("]");}
        printf("],\"after\":[");
        for(k=0;k<4;++k) {printf("%s[",k?",":"");terrain_node_print(node[k]);printf("]");}
        {
            unsigned long mid,a,b,first;
            memcpy(&first,edge,4);
            printf("],\"edge\":{\"head\":%d",first==(unsigned long)node[0]?0:1);
            memcpy(&mid,edge+8,4);
            printf(",\"mid\":");
            if(mid==0) printf("null,\"node\":[]");
            else if(mid==(unsigned long)node[3]) printf("3,\"node\":[]");
            else {printf("1,\"node\":[");terrain_node_print((unsigned char *)mid);printf("]");}
            memcpy(&a,edge+4,4);memcpy(&b,edge+0xc,4);
            printf(",\"links\":[");
            link=(unsigned char *)a;
            for(k=0;k<2;++k) {
                unsigned long target=0;int id=-1;
                if(link) {memcpy(&target,link,4);id=target==(unsigned long)node[0]?0:target==(unsigned long)node[1]?1:target==(unsigned long)node[2]?2:9;}
                printf("%s%d",k?",":"",id);
                link=(unsigned char *)b;
            }
            printf("]}");
        }
        printf(",\"nodes\":%lu,\"edges\":%lu}\n",used_nodes,used_edges);
    }
    if (fflush(stdout)) fail("cannot flush terrain-mid vectors");
    return 0;
}

/* The terrain query. terrain drives the original F1876 in distance mode on a
   synthetic planet: the object's seed, a feature stream (records at random places
   of the face tree), a rotated and scaled orientation, a translation near and far
   along z and a detail setting. It prints the inputs and the distance, the
   stream bytes consumed, the raw distance, the arena counts and the layer tables
   afterwards. Divisions by zero in the original are caught and skipped. */
extern long ref_terrain(long *result,long max,unsigned char *object,long p4,unsigned char *stream,long *translation,long *matrix,long kind,long u1,long u2,long u3,long mode);
extern long ref_detail,ref_tr_raw;
static sigjmp_buf terrain_jump;
static void terrain_fault(int signal_number) {(void)signal_number;siglongjmp(terrain_jump,1);}
static double terrain_unit(void) {return (double)texture_random(0,1000000)/1000000.0;}
static void terrain_rotation(double r[9])
{
    double a[3],b[3],c[3],n;
    int k;
    do {
        for(k=0;k<3;++k) {a[k]=terrain_unit()-0.5;b[k]=terrain_unit()-0.5;}
        n=sqrt(a[0]*a[0]+a[1]*a[1]+a[2]*a[2]);
    } while(n<0.1);
    for(k=0;k<3;++k) a[k]/=n;
    n=a[0]*b[0]+a[1]*b[1]+a[2]*b[2];
    for(k=0;k<3;++k) b[k]-=n*a[k];
    n=sqrt(b[0]*b[0]+b[1]*b[1]+b[2]*b[2]);
    if(n<0.05) {b[0]=-a[1];b[1]=a[0];b[2]=0;n=sqrt(b[0]*b[0]+b[1]*b[1]);if(n<0.01) {b[0]=0;b[1]=-a[2];b[2]=a[1];n=sqrt(b[1]*b[1]+b[2]*b[2]);}}
    for(k=0;k<3;++k) b[k]/=n;
    c[0]=a[1]*b[2]-a[2]*b[1];c[1]=a[2]*b[0]-a[0]*b[2];c[2]=a[0]*b[1]-a[1]*b[0];
    for(k=0;k<3;++k) {r[k]=a[k];r[3+k]=b[k];r[6+k]=c[k];}
}
static int terrain_vectors(void)
{
    static unsigned char object[0x200];
    unsigned char stream[16*100+16];
    unsigned long n,found_cases=0;
    fprintf(stderr,"JJFFE terrain-v1: original F1876 in distance mode (F1823, F1833, F1834, F1875, F1816-F1821, F1849-F1867) for synthetic planets: seed, feature stream, orientation, translation, planet type and detail.\n");
    texture_rng=0x2e8b5a31UL;
    signal(SIGFPE,terrain_fault);
    for(n=0;n<2048;++n) {
        unsigned long seed=((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff);
        long matrix[9],translation[3],kind=texture_random(0,13),detail=texture_random(0,4),max,result=0x55555555;
        long records=texture_random(0,3)==0?0:texture_random(1,10),k,used=0;
        unsigned long keys[16];
        double rotation[9],radius=(double)(1UL<<texture_random(19,28))*(0.5+terrain_unit());
        long distance;
        long consumed,fault;
        /* Types 3, 5 and 6 select a routine pair past the table when a face has features. */
        if(kind==3||kind==5||kind==6) records=0;
        memset(object,0,sizeof(object));memset(stream,0,sizeof(stream));
        memcpy(object+0xa0,&seed,4);
        for(k=0;k<records && used<10;++k) {
            long depth=texture_random(0,5),face=texture_random(0,19),m,i,clash=0;
            /* A record at the root is an ancestor of every other: only the last may be. */
            if(depth==0 && k!=records-1) depth=1;
            unsigned long key,bits;
            if(depth==0) key=(unsigned long)texture_random(1,15);
            else {
                m=2*depth+3;
                bits=((unsigned long)face<<(2*(depth-1)));
                for(i=0;i<depth-1;++i) bits|=((unsigned long)texture_random(0,3))<<(2*i);
                key=(1UL<<m)|bits;
            }
            for(i=0;i<used;++i) {
                /* One key's path must not lie on another's (the original would read garbage). */
                unsigned long x=key,y=keys[i];
                while(x>y && x>1) x>>=2;
                if(x==y) clash=1;
                x=key;
                while(y>x && y>1) y>>=2;
                if(x==y) clash=1;
            }
            if(clash) continue;
            keys[used]=key;
            {
                short w[8];
                long j;
                w[0]=(short)key;w[1]=12;
                for(j=2;j<8;++j) w[j]=texture_random(0,3)==0?(short)texture_random(-200,300):(short)texture_random(0,0x7f);
                memcpy(stream+16*used,w,16);
            }
            ++used;
        }
        if(!(kind==3||kind==5||kind==6) && texture_random(0,2)==0) {
            /* Every face carries features: a record at the face or at its four sub-triangles. */
            long face,child,w;
            short rec[8];
            memset(stream,0,sizeof(stream));
            used=0;
            for(face=0;face<20;++face) {
                int whole=texture_random(0,9)<7;
                for(child=0;child<(whole?1:4);++child) {
                    rec[0]=whole?(short)(32+face):(short)(128+4*face+child);
                    rec[1]=12;
                    for(w=2;w<8;++w) rec[w]=texture_random(0,3)==0?(short)texture_random(-200,300):(short)texture_random(0,0x7f);
                    memcpy(stream+16*used,rec,16);
                    ++used;
                }
            }
        }
        consumed=16*used+2;
        terrain_rotation(rotation);
        for(k=0;k<9;++k) matrix[k]=(long)(rotation[k]*radius);
        translation[0]=texture_random(0,2)==0?(long)((terrain_unit()-0.5)*radius*(texture_random(0,2)==0?5.0:1.0)):0;
        translation[1]=texture_random(0,2)==0?(long)((terrain_unit()-0.5)*radius*(texture_random(0,2)==0?5.0:1.0)):0;
        translation[2]=texture_random(0,2)==0?(long)(radius*(1.0+(terrain_unit()-0.5)*ldexp(1.0,-(int)texture_random(4,18)))):texture_random(0,1)?(long)(radius*(0.93+0.12*terrain_unit())):(long)((texture_random(0,5)==0?-1.0:1.0)*radius*(0.4+3.0*terrain_unit()));
        max=texture_random(0,4)==0?0x7fffffffL:(long)(radius*(0.5+3.0*terrain_unit()));
        ref_detail=detail;ref_tr_raw=0x12345678;
        memset(ref_tr_pool,0,sizeof(ref_tr_pool));memset(ref_tr_edge_pool,0,sizeof(ref_tr_edge_pool));
        memset(ref_tr_layers,0,sizeof(unsigned long)*128);
        ref_tr_nodes_used=0;ref_tr_edges_used=0;
        fault=sigsetjmp(terrain_jump,1);
        if(!fault) {
            distance=(long)((unsigned char *)ref_terrain(&result,max,object,0,stream,translation,matrix,kind,0,0,0,1)-stream);
        } else distance=0;
        printf("{\"case\":%lu,\"seed\":%lu,\"max\":%ld,\"detail\":%ld,\"kind\":%ld,\"translation\":[%ld,%ld,%ld],\"matrix\":[",n,seed,max,detail,kind,translation[0],translation[1],translation[2]);
        for(k=0;k<9;++k) printf("%s%ld",k?",":"",matrix[k]);
        printf("],\"stream\":[");
        for(k=0;k<consumed;++k) printf("%s%u",k?",":"",stream[k]);
        if(fault) {printf("],\"fault\":true}\n");continue;}
        printf("],\"fault\":false,\"value\":%ld,\"consumed\":%ld,\"raw\":%ld,\"nodes\":%lu,\"edges\":%lu,\"layers\":[",result,distance,(long)ref_tr_raw,ref_tr_nodes_used,ref_tr_edges_used);
        for(k=0;k<128;++k) printf("%s%d",k?",":"",ref_tr_layers[k]==0?0:ref_tr_layers[k]==(unsigned long)ref_tr_map0?1:ref_tr_layers[k]==(unsigned long)ref_tr_map1?2:9);
        printf("]}\n");
        found_cases+=(ref_tr_raw!=0x7fffffffL);
    }
    fprintf(stderr,"terrain: %lu of 2048 cases found a face\n",found_cases);
    if (fflush(stdout)) fail("cannot flush terrain vectors");
    return 0;
}

/* Curved surfaces in draw mode. surface_draw drives the original F1876 with its
   twelfth argument 0, as F1874 calls it, for planet-scale patches: the surface
   record of one of the planet models (type, colour, parameters, feature records)
   or a random type and records, a random orientation and distance, the instance's
   scale, zoom, light and colours, and the detail setting. The probe build hooks the
   call every F1868/F1869 makes to the emitter (DATA_009270) and records what it is
   handed; the words F1876 leaves in the order queue are printed too, with the draw
   routines' addresses replaced by 0xf18000nn. */
#ifdef FFE_COMMAND_PROBE
extern void (*ref_emit)(void *,void *,void *,long *,long);
extern long ref_emit_level;
extern unsigned char ref_det_a[],ref_det_b[];
extern unsigned long ref_order_head;
extern unsigned char ref_group_open[],ref_group_close[],ref_ring_quad[],ref_sky[];
extern unsigned long ref_haze_flag,ref_haze_distance,ref_haze_colour,ref_haze_scale,ref_sky_flag;
static unsigned long draw_emits;
static uint64_t draw_hash;
static unsigned long draw_first[48][35];
static int draw_dump,draw_overflow;
static FILE *draw_out;
static void draw_mix(unsigned long v)
{
    int i;
    for(i=0;i<4;++i) {draw_hash^=(v>>(8*i))&0xff;draw_hash*=UINT64_C(1099511628211);}
}
static int draw_signal;
static void draw_fault(int signal_number) {draw_signal=signal_number;siglongjmp(terrain_jump,1);}
void trace_queue_overflow(void)
{
    draw_overflow=1;
    siglongjmp(terrain_jump,1);
}
void trace_surface_emit(unsigned char *a,unsigned char *b,unsigned char *c,long *vec,long key)
{
    unsigned char *nodes[3];
    unsigned long row[35],k=0,i;
    nodes[0]=a;nodes[1]=b;nodes[2]=c;
    for(i=0;i<3;++i) {
        unsigned char *p=nodes[i];
        row[k++]=(unsigned long)(long)*(short *)(p+0);
        row[k++]=(unsigned long)(long)*(short *)(p+2);
        row[k++]=*(unsigned long *)(p+4);
        row[k++]=*(unsigned long *)(p+8);
        row[k++]=*(unsigned long *)(p+0xc);
        row[k++]=(unsigned long)(long)*(short *)(p+0x24);
        row[k++]=(unsigned long)(long)*(short *)(p+0x26);
        row[k++]=*(unsigned long *)(p+0x3c);
        row[k++]=*(unsigned long *)(p+0x40);
        row[k++]=*(unsigned long *)(p+0x44);
    }
    row[k++]=(unsigned long)vec[0];row[k++]=(unsigned long)vec[1];row[k++]=(unsigned long)vec[2];
    row[k++]=(unsigned long)key;row[k++]=(unsigned long)ref_emit_level;
    for(i=0;i<k;++i) draw_mix(row[i]);
    if(draw_emits<48) memcpy(draw_first[draw_emits],row,sizeof(row));
    if(draw_dump) {
        fprintf(draw_out,"%s[",draw_emits?",":"");
        for(i=0;i<k;++i) fprintf(draw_out,"%s%ld",i?",":"",(long)row[i]);
        fprintf(draw_out,"]");
    }
    ++draw_emits;
    ref_emit(a,b,c,vec,key);
}
static const int surface_models[39]={113,114,115,116,117,118,119,120,121,122,123,124,125,126,127,128,129,130,131,132,133,134,135,136,137,138,139,140,141,142,143,144,145,146,147,148,445,447,449};
static unsigned char draw_link[65536+8];
static unsigned long draw_handler(unsigned long word,unsigned long position)
{
    if(word==(unsigned long)ref_flat_draw_triangle) return 0xf1800004UL;
    if(word==(unsigned long)ref_tex_draw_quad) return 0xf1800005UL;
    if(word==(unsigned long)ref_det_a) return 0xf1800006UL;
    if(word==(unsigned long)ref_tex_draw_triangle) return 0xf1800007UL;
    if(word==(unsigned long)ref_det_b) return 0xf1800008UL;
    if(word==(unsigned long)ref_flat_draw_quad) return 0xf1800009UL;
    if(word==(unsigned long)ref_ring_quad) return 0xf180000aUL;
    if(word==(unsigned long)ref_group_open) return 0xf1800010UL;
    if(word==(unsigned long)ref_group_close) return 0xf1800011UL;
    /* A header's link to the previous header: that header's position. */
    if(position<65536 && draw_link[position] && word) return 0xf2000000UL|((word-(unsigned long)ref_order_base)/4);
    return word;
}
static void draw_mark_links(void)
{
    unsigned long header=ref_order_head,guard=0;
    memset(draw_link,0,sizeof(draw_link));
    while(header && guard++<70000) {
        unsigned long index=(header-(unsigned long)ref_order_base)/4;
        if(index>=65536) break;
        draw_link[index+1]=1;
        header=ref_order_base[index+1];
    }
}
#endif
static int surface_draw_vectors(int only)
{
#ifndef FFE_COMMAND_PROBE
    (void)only;
    fail("--surface-draw requires the separate instrumented jjffe-commands.exe");
    return 2;
#else
    static unsigned char object[0x200],normals[32768*8];
    static unsigned long model_queue[65536];
    static unsigned long instance[0x160/4],globals[0x120/4],slots[128*9];
    static const long identity[9]={2147483647,0,0,0,2147483647,0,0,0,2147483647};
    unsigned char stream[16*100+16];
    unsigned long n,drew=0;
    fprintf(stderr,"JJFFE surface-draw-v1: original F1876 in draw mode (F1692, F1691, F1877, F1833, F1834, F1868/F1869, the emitter F1840/F1841 and its helpers) for planet-scale patches: a planet model's surface record or a random type and feature records, orientation, distance, instance scale, zoom, light and colours, and detail. Prints the emitter's arguments and the order queue.\n");
    texture_rng=0x3c6ef372UL;
    signal(SIGFPE,draw_fault);
    signal(SIGSEGV,draw_fault);
    for(n=0;n<512;++n) {
        unsigned long seed=((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff);
        unsigned long *model;
        unsigned char *program;
        long index=surface_models[texture_random(0,38)];
        long word0,kind,word8,word10,records=0,used=0,k,fault;
        long origin[3],vectors[9],detail=texture_random(-1,4);
        double rotation[9],radius=(double)(1UL<<texture_random(21,28))*(0.5+terrain_unit()),zf;
        unsigned char sky_in[3];
        unsigned long keys[16],consumed=0,words,i;
        int own=texture_random(0,9)<7;
        model=ref_model(index);
        program=(unsigned char *)model[0];
        word0=(short)(program[2]|program[3]<<8);
        kind=(short)(program[2+6]|program[2+7]<<8);
        word8=(short)(program[2+8]|program[2+9]<<8);
        word10=(short)(program[2+10]|program[2+11]<<8);
        memset(stream,0,sizeof(stream));
        if(own) {
            /* The model's own feature records, up to the zero key. */
            const unsigned char *q=program+2+12;
            while((q[0]|q[1]<<8)!=0 && used<99) {memcpy(stream+16*used,q,16);q+=16;++used;}
        } else {
            kind=texture_random(0,13);
            records=(kind==3||kind==5||kind==6||texture_random(0,3)==0)?0:texture_random(1,8);
            for(k=0;k<records && used<10;++k) {
                long depth=texture_random(0,5),face=texture_random(0,19),m,j,clash=0;
                unsigned long key,bits;
                if(depth==0 && k!=records-1) depth=1;
                if(depth==0) key=(unsigned long)texture_random(1,15);
                else {
                    m=2*depth+3;
                    bits=((unsigned long)face<<(2*(depth-1)));
                    for(j=0;j<depth-1;++j) bits|=((unsigned long)texture_random(0,3))<<(2*j);
                    key=(1UL<<m)|bits;
                }
                for(j=0;j<used;++j) {
                    unsigned long x=key,y=keys[j];
                    while(x>y && x>1) x>>=2;
                    if(x==y) clash=1;
                    x=key;
                    while(y>x && y>1) y>>=2;
                    if(x==y) clash=1;
                }
                if(clash) continue;
                keys[used]=key;
                {
                    short w[8];
                    long j2;
                    w[0]=(short)key;w[1]=12;
                    for(j2=2;j2<8;++j2) w[j2]=texture_random(0,3)==0?(short)texture_random(-200,300):(short)texture_random(0,0x7f);
                    memcpy(stream+16*used,w,16);
                }
                ++used;
            }
            word0=texture_random(0,3)==0?(long)(short)texture_random(0,0x3fff):word0;
            word8=texture_random(0,2)==0?0:(long)texture_random(1,0x2000);
            word10=texture_random(0,8);
        }
        consumed=16*used+2;
        terrain_rotation(rotation);
        for(k=0;k<9;++k) vectors[k]=(long)(rotation[k]*radius);
        switch(texture_random(0,4)) {
        case 0: zf=1.02+0.5*terrain_unit(); break;
        case 1: case 2: zf=1.5+3.0*terrain_unit(); break;
        case 3: zf=4.0+16.0*terrain_unit(); break;
        default: zf=1.0+terrain_unit()*60.0; break;
        }
        origin[2]=(long)(radius*zf);
        origin[0]=(long)((terrain_unit()-0.5)*radius*zf*(texture_random(0,2)?0.6:2.0));
        origin[1]=(long)((terrain_unit()-0.5)*radius*zf*(texture_random(0,2)?0.4:1.5));
        memset(instance,0,sizeof(instance));memset(globals,0,sizeof(globals));memset(object,0,sizeof(object));
        memset(slots,0,sizeof(slots));memset(normals,0,sizeof(normals));
        memcpy((char *)instance+0x14,identity,36);
        memcpy((char *)instance+0x9c,identity,36);
        instance[0xc/4]=2000;
        instance[0x148/4]=model[5]+(texture_random(0,5)==0?(unsigned long)texture_random(-2,2):0);
        instance[0xf0/4]=2;
        instance[0x14c/4]=(unsigned long)object;
        instance[0x150/4]=model[1];
        instance[0x154/4]=model[3];
        instance[0x130/4]=texture_random(0,4)==0?(unsigned long)texture_random(1,5):0;
        {
            double lv[3],ln;
            unsigned char *shades=(unsigned char *)instance+0x84;
            do {for(k=0;k<3;++k) lv[k]=terrain_unit()-0.5; ln=sqrt(lv[0]*lv[0]+lv[1]*lv[1]+lv[2]*lv[2]);} while(ln<0.1);
            for(k=0;k<3;++k) instance[0x134/4+k]=(unsigned long)(long)(lv[k]/ln*2147483647.0);
            for(k=0;k<24;++k) shades[k]=(unsigned char)texture_random(0,15);
        }
        memcpy(object+0xa0,&seed,4);
        ref_vertex_buffer=(long *)slots;
        ref_normal_buffer=normals;
        ref_detail_primary=ref_detail_secondary=0;
        ref_detail=detail;ref_tr_raw=0x12345678;
        memset(ref_tr_pool,0,sizeof(ref_tr_pool));memset(ref_tr_edge_pool,0,sizeof(ref_tr_edge_pool));
        memset(ref_tr_layers,0,sizeof(unsigned long)*128);
        ref_tr_nodes_used=0;ref_tr_edges_used=0;
        ref_haze_flag=ref_haze_distance=ref_haze_colour=ref_haze_scale=0;ref_sky_flag=0;
        for(k=0;k<3;++k) {sky_in[k]=(unsigned char)texture_random(0,15);ref_sky[k]=sky_in[k];}
        memset(model_queue,0,sizeof(model_queue));
        ref_order_base=model_queue;ref_order_active=0;ref_order_state=0;
        ref_order_init(model_queue);
        ref_jitter_a=0x12345678UL;ref_jitter_b=0x456789abUL;
        draw_emits=0;draw_hash=UINT64_C(14695981039346656037);
        memset(draw_first,0,sizeof(draw_first));
        draw_dump=(only==(int)n);
        if(only>=0 && !draw_dump) continue;
        {
            /* Each case runs in a child: an overflow or a fault leaves the original's
               state unusable, and a crash must not end the run. */
            int status=0;
            pid_t child;
            fflush(stdout);
            child=fork();
            if(child<0) fail("fork failed");
            if(child>0) {
                waitpid(child,&status,0);
                if(!(WIFEXITED(status) && WEXITSTATUS(status)==0)) printf("{\"case\":%lu,\"fault\":true,\"crashed\":true}\n",n);
                continue;
            }
        }
        {
            char *buffer=0;size_t size=0;
            draw_out=open_memstream(&buffer,&size);
        fprintf(draw_out,"{\"case\":%lu,\"model\":%ld,\"seed\":%lu,\"word0\":%ld,\"kind\":%ld,\"word8\":%ld,\"word10\":%ld,\"detail\":%ld,\"scale\":%ld,\"zoom\":%ld,\"origin\":[%ld,%ld,%ld],\"vectors\":[",n,index,seed,word0,kind,word8,word10,detail,(long)instance[0x148/4],(long)instance[0x130/4],origin[0],origin[1],origin[2]);
        for(k=0;k<9;++k) fprintf(draw_out,"%s%ld",k?",":"",vectors[k]);
        fprintf(draw_out,"],\"sky\":[%u,%u,%u],\"light\":[%ld,%ld,%ld],\"shades\":[",sky_in[0],sky_in[1],sky_in[2],(long)instance[0x134/4],(long)instance[0x138/4],(long)instance[0x13c/4]);
        for(k=0;k<24;++k) fprintf(draw_out,"%s%u",k?",":"",((unsigned char *)instance)[0x84+k]);
        fprintf(draw_out,"],\"stream\":[");
        for(k=0;k<(long)consumed;++k) fprintf(draw_out,"%s%u",k?",":"",stream[k]);
        fprintf(draw_out,"],\"all\":[");
        draw_overflow=0;draw_signal=0;
        fault=sigsetjmp(terrain_jump,1);
        if(!fault) {
            long result=0x55555555;
            ref_surface((long)&result,0,(long)object,(long)instance,(long)stream,(long)origin,(long)vectors,kind,word10,word8<<16,word0,0);
        }
        if(draw_dump) fprintf(draw_out,"]");
        words=(unsigned long)(ref_order_cursor-ref_order_base);
        if(fault) {fprintf(draw_out,"%s\"fault\":true,\"overflow\":%d,\"signal\":%d}\n",draw_dump?",":"],",draw_overflow,draw_signal);fclose(draw_out);fwrite(buffer,1,size,stdout);fflush(stdout);_exit(0);}
        fprintf(draw_out,"%s\"fault\":false,\"emits\":%lu,\"hash\":\"%016llx\",\"nodes\":%lu,\"edges\":%lu,\"first\":[",draw_dump?",":"],",draw_emits,(unsigned long long)draw_hash,ref_tr_nodes_used,ref_tr_edges_used);
        for(i=0;i<draw_emits && i<48;++i) {
            unsigned long j;
            fprintf(draw_out,"%s[",i?",":"");
            for(j=0;j<35;++j) fprintf(draw_out,"%s%ld",j?",":"",(long)draw_first[i][j]);
            fprintf(draw_out,"]");
        }
        {
            uint64_t queue_hash=UINT64_C(14695981039346656037);
            draw_mark_links();
            for(i=0;i<words;++i) {
                unsigned long w=draw_handler(ref_order_base[i],i);
                int j;
                for(j=0;j<4;++j) {queue_hash^=(w>>(8*j))&0xff;queue_hash*=UINT64_C(1099511628211);}
            }
            fprintf(draw_out,"],\"haze\":[%ld,%ld,%lu,%ld],\"sky_out\":[%ld,%u,%u,%u],\"words\":%lu,\"queue_hash\":\"%016llx\",\"queue\":[",(long)ref_haze_flag,(long)ref_haze_distance,ref_haze_colour,(long)ref_haze_scale,(long)ref_sky_flag,ref_sky[0],ref_sky[1],ref_sky[2],words,(unsigned long long)queue_hash);
            for(i=0;i<words && (i<600 || draw_dump);++i) fprintf(draw_out,"%s%lu",i?",":"",draw_handler(ref_order_base[i],i));
        }
        fprintf(draw_out,"]}\n");
        fclose(draw_out);fwrite(buffer,1,size,stdout);fflush(stdout);_exit(0);
        }
    }
    (void)drew;
    if (fflush(stdout)) fail("cannot flush surface-draw vectors");
    return 0;
#endif
}

/* Shape collisions. planet drives the original F576 (with F579, F580, F581,
   F582, F583 and, for a planet's terrain record, F1876) for a ship sphere against
   a model's collision stream: any of the game's models stands in as the complex
   shape, with a rotation and radii from its extent, the ship at distances from
   0.2 to 3 radii. It prints the inputs and the return value and registers. */
extern long ref_hit(unsigned char *ship,unsigned char *planet,long long *relpos);
extern unsigned char ref_coll_flags,ref_c_more,ref_c_object,ref_c_chunk,ref_c_planet;
extern unsigned long ref_c_altitude[2];
extern unsigned long ref_coll_distance,ref_coll_distance_high;
static sigjmp_buf planet_jump;
static void planet_fault(int signal_number) {(void)signal_number;siglongjmp(planet_jump,1);}
static void planet_matrix(unsigned char *object)
{
    double r[9];
    long k;
    terrain_rotation(r);
    for(k=0;k<9;++k) put32(object,4*k,(long)(r[k]*2147483647.0));
}
static double planet_extent(const unsigned long *model)
{
    const signed char *vertices=(const signed char *)model[1];
    unsigned long count=model[2]/2,k;
    double best=1.0;
    for(k=0;k<count;++k) {
        double x=vertices[6*k+2],y=vertices[6*k+3],z=vertices[6*k+4],r=sqrt(x*x+y*y+z*z);
        if(r>best) best=r;
    }
    return best;
}
/* A body made into a planet: a planet model standing for the complex shape, radii
   from its extent, a rotation and a seed (the animation words are the object's own
   bytes). */
static void planet_parent(unsigned char *object)
{
    int model=(int)texture_random(113,148);
    unsigned long *m=ref_model(model);
    double radius=planet_extent(m)*ldexp(1.0,(int)(m[0x14/4]+m[0x18/4]));
    long long outer=(long long)(radius*(1.0+0.5*terrain_unit())),inner=(long long)(radius*(0.8+0.2*terrain_unit()));
    unsigned long seed=((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff);
    put32(object,0x82,(long)model);
    object[0x14c]=0x80;
    memcpy(object+0x138,&outer,8);memcpy(object+0x140,&inner,8);
    memcpy(object+0xa0,&seed,4);
    planet_matrix(object);
}

static int has_shape(unsigned long model)
{
    unsigned long *m;
    if(model>=466) return 0;
    m=ref_model((int)model);
    return m && m[0x34/4];
}

static int planet_vectors(void)
{
    static unsigned char ship[0x200],planet[0x200];
    int shapes[466],chained[466],count=0,chains=0;
    unsigned long n,i,k;
    fprintf(stderr,"JJFFE planet-v1: original F576 (F579, F580, F581, F582, F583, F1876) for a ship sphere against the collision stream of any model standing in as the complex shape, with random rotations, radii from the model's extent, positions 0.2 to 3 radii away, animation words and a detail setting: the return value, the registers and the stream entry that decided.\n");
    for(i=0;i<466;++i) {unsigned long *m=ref_model((int)i); if(m && m[0x34/4]) {shapes[count++]=(int)i;if(*(unsigned short *)(m[0x34/4]-2)) chained[chains++]=(int)i;}}
    texture_rng=0x4c8f3d11UL;
    signal(SIGSEGV,planet_fault);signal(SIGFPE,planet_fault);
    for(n=0;n<4096;++n) {
        int pm=texture_random(0,1)?(int)texture_random(113,148):shapes[texture_random(0,count-1)],sm=texture_random(0,2)==0?chained[texture_random(0,chains-1)]:shapes[texture_random(0,count-1)];
        unsigned long *pmodel=ref_model(pm),*smodel=ref_model(sm);
        double radius,length,direction[3],reach;
        long long relpos[3],outer,inner,ship_inner,ship_outer;
        long detail=texture_random(0,4);
        unsigned long seed=((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff);
        int fault,result=0,complex_ship=texture_random(0,3)!=0;
        memset(ship,0,sizeof(ship));memset(planet,0,sizeof(planet));
        radius=planet_extent(pmodel)*ldexp(1.0,(int)(pmodel[0x14/4]+pmodel[0x18/4]));
        /* the basis vectors of a terrain record span the radius; a plain model's extent is its own */
        outer=(long long)(radius*(1.0+0.5*terrain_unit()));
        inner=(long long)(radius*(0.8+0.2*terrain_unit()));
        if(texture_random(0,3)==0) inner=outer;
        ship_inner=(long long)(radius*0.05*terrain_unit());
        ship_outer=ship_inner+(texture_random(0,1)?0:(long long)(ship_inner*0.1));
        for(k=0;k<0x5b;++k) {unsigned short w=(unsigned short)texture_random(0,0xffff);memcpy(planet+0x9c+2*k,&w,2);}
        put32(planet,0x82,(long)pm);planet[0x86]=5;
        memcpy(planet+0x138,&outer,8);memcpy(planet+0x140,&inner,8);
        planet[0x14c]=0x80;
        memcpy(planet+0xa0,&seed,4);
        planet_matrix(planet);
        put32(ship,0x82,(long)sm);ship[0x86]=7;
        memcpy(ship+0x138,&ship_outer,8);memcpy(ship+0x140,&ship_inner,8);
        ship[0x14c]=complex_ship?0x80:0;
        ship[0x56]=(unsigned char)(texture_random(0,1)?5:4);ship[0x57]=(unsigned char)texture_random(0,1);
        planet_matrix(ship);
        for(k=0;k<3;++k) direction[k]=terrain_unit()-0.5;
        length=sqrt(direction[0]*direction[0]+direction[1]*direction[1]+direction[2]*direction[2])+1e-9;
        reach=radius*(texture_random(0,1)?(0.2+2.8*terrain_unit()):(0.85+0.4*terrain_unit()));
        for(k=0;k<3;++k) relpos[k]=(long long)(direction[k]/length*reach);
        ref_detail=detail;
        ref_coll_flags=0;ref_c_more=0;ref_c_object=0;ref_c_chunk=0;ref_c_planet=0;
        ref_coll_distance=0xffffffffUL;ref_coll_distance_high=0;ref_c_altitude[0]=0;ref_c_altitude[1]=0;
        memset(ref_tr_pool,0,sizeof(ref_tr_pool));memset(ref_tr_edge_pool,0,sizeof(ref_tr_edge_pool));
        memset(ref_tr_layers,0,sizeof(unsigned long)*128);
        fault=sigsetjmp(planet_jump,1);
        if(!fault) result=(int)(signed char)ref_hit(ship,planet,relpos);
        printf("{\"case\":%lu,\"planet\":%d,\"ship\":%d,\"detail\":%ld,\"seed\":%lu,\"outer\":%lld,\"inner\":%lld,\"ship_outer\":%lld,\"ship_inner\":%lld,\"complex\":%d,\"parent\":%u,\"rotating\":%u,\"words\":[",n,pm,sm,detail,seed,outer,inner,ship_outer,ship_inner,complex_ship,(unsigned)ship[0x56],(unsigned)ship[0x57]);
        for(k=0;k<0x5b;++k) printf("%s%u",k?",":"",(unsigned)*(unsigned short *)(planet+0x9c+2*k));
        printf("],\"planet_matrix\":[");
        for(k=0;k<9;++k) printf("%s%ld",k?",":"",get32(planet,4*k));
        printf("],\"ship_matrix\":[");
        for(k=0;k<9;++k) printf("%s%ld",k?",":"",get32(ship,4*k));
        printf("],\"relpos\":[%lld,%lld,%lld],",relpos[0],relpos[1],relpos[2]);
        if(fault) {printf("\"fault\":true}\n");continue;}
        printf("\"fault\":false,\"result\":%d,\"flags\":%u,\"more\":%u,\"object\":%u,\"chunk\":%u,\"planet_flag\":%u,\"distance\":%lld,\"altitude\":%lld}\n",result,(unsigned)ref_coll_flags,(unsigned)ref_c_more,(unsigned)ref_c_object,(unsigned)ref_c_chunk,(unsigned)ref_c_planet,(long long)(((unsigned long long)ref_coll_distance_high<<32)|ref_coll_distance),(long long)(((unsigned long long)ref_c_altitude[1]<<32)|ref_c_altitude[0]));
    }
    if (fflush(stdout)) fail("cannot flush planet vectors");
    return 0;
}

/* Name generation. names drives the original F791 (F792-F800, F921, F1348) for
   the name types that need no game state, with random seeds. */
extern unsigned char *ref_name(long kind,unsigned char *vars,unsigned long seed,unsigned char *out);
static int name_vectors(void)
{
    static const int kinds[]={1,2,3,4,5,6,7,8,9,10,11,12,13,14,0x10,0x11,0x12,0x13,0x14,0,0xf};
    unsigned long n,k;
    fprintf(stderr,"JJFFE names-v1: original F791 (F792-F800, F921 and the string expander) for name types 1 to 0x14 with random seeds: the text written and its length.\n");
    texture_rng=0x1d872b41UL;
    for(n=0;n<8192;++n) {
        unsigned char vars[0x20],out[0x200];
        long kind=kinds[n%(sizeof(kinds)/sizeof(kinds[0]))];
        unsigned long seed=((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff);
        unsigned char *end;
        memset(vars,0,sizeof(vars));memset(out,0xaa,sizeof(out));
        end=ref_name(kind,vars,seed,out);
        printf("{\"case\":%lu,\"kind\":%ld,\"seed\":%lu,\"length\":%ld,\"text\":[",n,kind,seed,(long)(end-out));
        for(k=0;k<(unsigned long)(end-out);++k) printf("%s%u",k?",":"",out[k]);
        printf("]}\n");
    }
    if (fflush(stdout)) fail("cannot flush name vectors");
    return 0;
}

/* Steering input. steer drives the original F1037 (set speed from Return and
   Shift through F1121, then F1038: mouse and key deltas into the rates, or the
   turret angles in a turret view) on the player struct (DATA_008804) and a
   zeroed object, with the key accumulator table, key bytes, input time and mouse
   global set per case; the state before and after is printed. */
extern void ref_steer(unsigned char *object);
extern unsigned char ref_player_struct[0x4000],ref_st_active,ref_pl_keys[0x60];
extern unsigned long ref_st_accum[0x100],ref_st_keys_on,ref_st_time,ref_st_boost,ref_st_mouse;
static int steer_vectors(void)
{
    static unsigned char object[0x400];
    unsigned long n,i;
    static const int codes[6]={0x2c,0x1e,0x33,0x34,0x1a,0x1b};
    unsigned long hold[6];
    unsigned char before_p[0x400],before_o[0x400];
    int return_key,shift_key;
    fprintf(stderr,"JJFFE steer-v1: original F1037 (F1121 set-speed steps, then F1038 heading input via the mouse accumulator DATA_009188 and the key accumulator table) on the player struct DATA_008804 and a zeroed object; the set speed, flags, mouse word, view, turret angles and rates before and after.\n");
    texture_rng=0x4a7c15e1UL;
    for(n=0;n<4096;++n) {
        memset(object,0,sizeof(object));
        memset(ref_player_struct,0,0x4000);
        memset(ref_st_accum,0,sizeof(unsigned long)*0x100);
        memset(ref_pl_keys,0x80,0x60);
        ref_st_active=(unsigned char)(texture_random(0,9)==0?0x7f:0xff);
        ref_st_keys_on=(unsigned long)texture_random(0,4)!=0;
        ref_st_time=(unsigned long)((n&1)?texture_random(0,30000):texture_random(0,5000000));
        ref_st_boost=(unsigned long)(texture_random(0,2)==0?texture_random(0,0x40):texture_random(0,0xffffff));
        {
            unsigned char fl=(unsigned char)texture_random(0,3)==0?(unsigned char)texture_random(0,255):(unsigned char)(texture_random(0,1)?0x80:0);
            ref_player_struct[0xe9]=fl;
        }
        ref_player_struct[0xea]=(unsigned char)texture_random(0,4);
        {long speed; switch(texture_random(0,4)) {
            case 0: speed=0; break;
            case 1: speed=texture_random(-100000,100000); break;
            case 2: speed=texture_random(-2000000000L,2000000000L); break;
            case 3: speed=0x7fffffffL-texture_random(0,100000); break;
            default: speed=-0x7fffffffL-1+texture_random(0,100000); break;}
         put32(ref_player_struct,0x1b8,speed);}
        put32(ref_player_struct,0xd0,(long)(((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff)));
        ref_st_mouse=(texture_random(0,2)==0)?(unsigned long)(((unsigned long)texture_random(0,0xffff)<<16)|(unsigned long)texture_random(0,0xffff)):(unsigned long)(*(unsigned long *)(ref_player_struct+0xd0)+(unsigned long)texture_random(-200,200)+((unsigned long)texture_random(-200,200)<<16));
        ref_player_struct[0x3f54]=(unsigned char)texture_random(0,1);
        ref_player_struct[0x3f55]=(unsigned char)texture_random(0,1);
        ref_player_struct[0x3f56]=(unsigned char)texture_random(0,1);
        {short w=(short)texture_random(-32768,32767); memcpy(ref_player_struct+0x204,&w,2);
         w=(short)texture_random(-32768,32767); memcpy(ref_player_struct+0x206,&w,2);}
        object[0xd1]=(unsigned char)(signed char)texture_random(-1,6);
        for(i=0;i<3;++i) {
            short rate=(short)(texture_random(0,2)==0?texture_random(-32768,32767):texture_random(-5000,5000));
            memcpy(object+0xb0+2*i,&rate,2);
        }
        for(i=0;i<6;++i) {
            hold[i]=(unsigned long)(texture_random(0,2)==0?0:texture_random(1,120));
            ref_st_accum[codes[i]]=hold[i];
        }
        return_key=texture_random(0,2)==0; shift_key=texture_random(0,2)==0;
        if(return_key) ref_pl_keys[0x1c]=0;
        if(shift_key) ref_pl_keys[0x36]=0;
        memcpy(before_p,ref_player_struct,sizeof(before_p));
        memcpy(before_o,object,sizeof(before_o));
        {
            unsigned long mouse=ref_st_mouse;
            int flip_x=ref_player_struct[0x3f54]==0,flip_y=ref_player_struct[0x3f55]==0,roll_mode=ref_player_struct[0x3f56]!=0;
            ref_steer(object);
            printf("{\"case\":%lu,\"active\":%u,\"keys_on\":%lu,\"time\":%lu,\"boost\":%lu,\"flags\":%u,\"view\":%u,\"speed\":%ld,\"previous\":%lu,\"mouse\":%lu,\"flip\":[%d,%d],\"roll\":%d,\"turret\":[%d,%d],\"gun_mounts\":%d,\"rate\":[",
                   n,ref_st_active,ref_st_keys_on,ref_st_time,ref_st_boost,before_p[0xe9],before_p[0xea],get32(before_p,0x1b8),
                   (unsigned long)get32(before_p,0xd0)&0xffffffffUL,mouse,flip_x,flip_y,roll_mode,
                   *(short *)(before_p+0x204),*(short *)(before_p+0x206),(int)(signed char)before_o[0xd1]);
            for(i=0;i<3;++i) printf("%s%d",i?",":"",*(short *)(before_o+0xb0+2*i));
            printf("],\"return\":%d,\"shift\":%d,\"hold\":[",return_key,shift_key);
            for(i=0;i<6;++i) printf("%s%lu",i?",":"",hold[i]);
            printf("],\"after\":{\"flags\":%u,\"view\":%u,\"speed\":%ld,\"previous\":%lu,\"turret\":[%d,%d],\"rate\":[",
                   ref_player_struct[0xe9],ref_player_struct[0xea],get32(ref_player_struct,0x1b8),
                   (unsigned long)get32(ref_player_struct,0xd0)&0xffffffffUL,
                   *(short *)(ref_player_struct+0x204),*(short *)(ref_player_struct+0x206));
            for(i=0;i<3;++i) printf("%s%d",i?",":"",*(short *)(object+0xb0+2*i));
            printf("]}}\n");
        }
    }
    if (fflush(stdout)) fail("cannot flush steer vectors");
    return 0;
}

static unsigned long texture_face_count;
static unsigned long texture_faces[1024][48];
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
    else if(face_entry && face_entry[0]==(unsigned long)ref_poly_start) {
        /* F1714: F1795's two points, each F1796's point, then F1799's colour. */
        unsigned long k=4;
        row[3]=3;
        if(face_entry[1]!=(unsigned long)ref_poly_edge) fail("F1714 entry without F1795");
        row[6]=face_entry[2];row[7]=face_entry[3];words=2;
        while(face_entry[k]==(unsigned long)ref_poly_to) {
            if(words>=40) fail("F1714 entry too long");
            row[6+words++]=face_entry[k+1];k+=2;
        }
        if(face_entry[k]!=(unsigned long)ref_poly_fill) fail("F1714 entry without F1799");
        row[6+words++]=face_entry[k+1];
    }
    row[4]=face_colour_called ? ((unsigned long)face_colour[0]|((unsigned long)face_colour[1]<<8)|((unsigned long)face_colour[2]<<16)) : 0xffffffffUL;
    row[5]=words;
    if(row[3]!=3) for(i=0;i<words;++i) row[6+i]=face_entry[1+i];
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
    fprintf(stderr,"JJFFE texture-queue-v1: one model slot through original F1772 with real F1708/F1712 (as station-model-v1, four views, instance colour 1,2,3, synthetic normal colours r=3i%%9 g=5i%%10 b=7i%%11, culled when i%%7==3); per face the queue entry of the textured (F1807/F1805), on-screen flat (F1804/F1809) or F1714 outline (path 3: its points, then the F1799 colour) path and the F1691 flat colour. Points/lines/text/children not queued; no flush or pixels.\n");
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


/* Traffic control oracle (--traffic): the original F654 (permission), F650 (a bay,
   indoor station), F651 (release), F652 (touch), F653 (reset), F655 (the whole request
   for a ship other than the player, so no message is printed), F657 (is docking) and
   F867's clearance countdown, on a synthetic station record. Each case starts from a
   random record (mode, current ship, timeout, bay count, bays) and applies one call;
   the record and the return values are printed. The player index is set to 0x7f so no
   ship in the cases is the player (F650/F655 raise events for the player). */
extern unsigned long ref_traffic_alloc(void *station, long ship, void *position, long *flag, long *pad);
extern void ref_traffic_release(long ship, void *station);
extern unsigned long ref_traffic_touch(long ship, void *station);
extern void ref_traffic_reset(void *station);
extern unsigned long ref_traffic_request(long ship, void *station);
extern unsigned long ref_traffic_dock(void *ship_object, void *station, void *position, long *pad);
extern unsigned long ref_traffic_docking(long ship, void *station);
extern void ref_traffic_elapse(void *parameters);
extern unsigned long ref_player_index;
static unsigned long traffic_state = 0x13579bdfUL;
static unsigned long traffic_random(void)
{
    traffic_state ^= traffic_state << 13; traffic_state &= 0xffffffffUL;
    traffic_state ^= traffic_state >> 17;
    traffic_state ^= traffic_state << 5; traffic_state &= 0xffffffffUL;
    return traffic_state;
}
static void traffic_print(const unsigned char *o)
{
    unsigned i;
    printf("\"mode\":%u,\"current\":%u,\"timeout\":%u,\"bays\":%u,\"pads\":[", o[0xc9], o[0xca], *(unsigned short *)(o + 0xd8), *(unsigned short *)(o + 0xce));
    for (i = 0; i < 8; ++i) printf("%s%u", i ? "," : "", o[0xd0 + i]);
    printf("]");
}
static int traffic_vectors(void)
{
    static unsigned char station[0x160];
    static unsigned char ship[0x160];
    static const unsigned char modes[] = {0, 1, 2, 3, 0xfe, 0xff, 4, 0x80};
    unsigned long i, k;
    fprintf(stderr, "JJFFE traffic-v1: original F650 (indoor bay), F651, F652, F653, F654, F655 (non-player ship), F657 and F867's clearance countdown on synthetic station records; the player index is 0x7f. No messages, events or open-air pads.\n");
    ref_player_index = 0x7f;
    for (i = 0; i < 8192; ++i) {
        unsigned long call = i % 8, ship_index, ca, elapsed;
        long flag = 0x5a5a5a5aL, pad = 0x5a5a5a5aL, position[6];
        unsigned long result = 0x5a5a5a5aUL;
        memset(station, 0, sizeof station);
        memset(ship, 0, sizeof ship);
        station[0xc9] = modes[traffic_random() % 8];
        ca = traffic_random() % 4 == 0 ? 0 : traffic_random() % 12 + 1;
        station[0xca] = (unsigned char)ca;
        *(unsigned short *)(station + 0xd8) = traffic_random() % 3 == 0 ? 0 : (unsigned short)(traffic_random() % 0x7100);
        *(unsigned short *)(station + 0xce) = (unsigned short)(traffic_random() % 8);
        for (k = 0; k < 8; ++k) station[0xd0 + k] = traffic_random() % 3 == 0 ? 0 : (unsigned char)(traffic_random() % 12 + 1);
        station[0x14c] = traffic_random() % 2 ? 0x10 : 0x00; /* indoor (bit 0x40 clear), not a starport (bit 0x20 clear) */
        *(unsigned long *)(station + 0x82) = 0x50;
        ship_index = traffic_random() % 12 + 1;
        ship[0x86] = (unsigned char)ship_index;
        elapsed = traffic_random() % 4 == 0 ? 0x7000 + traffic_random() % 0x200 : traffic_random() % 0x300;
        {
            unsigned char before[0x160];
            memcpy(before, station, sizeof station);
            printf("{\"call\":%lu,\"ship\":%lu,\"elapsed\":%lu,\"in\":{", call, ship_index, elapsed);
            traffic_print(before);
            printf("}");
        }
        switch (call) {
        case 0: result = ref_traffic_request((long)ship_index, station) & 0xff; break;
        case 1: result = ref_traffic_alloc(station, (long)ship_index, position, &flag, &pad) & 0xffffffffUL; break;
        case 2: ref_traffic_release((long)ship_index, station); break;
        case 3: result = ref_traffic_touch((long)ship_index, station) & 0xff; break;
        case 4: ref_traffic_reset(station); break;
        case 5: result = ref_traffic_dock(ship, station, position, &pad) & 0xff; break;
        case 6: result = ref_traffic_docking((long)ship_index, station) & 0xffffffffUL; break;
        default: {
            unsigned long parameters[2];
            parameters[0] = (unsigned long)station; parameters[1] = elapsed;
            ref_traffic_elapse(parameters);
        }
        }
        printf(",\"result\":%lu,\"flag\":%ld,\"pad\":%ld,\"out\":{", result, flag, pad);
        traffic_print(station);
        printf("}}\n");
    }
    if (fflush(stdout)) fail("cannot flush traffic vectors");
    return 0;
}


/* Open-air pad oracle (--pads): the original F650 on synthetic open-air and
   indoor station records. Each case fixes the ship's bay (empty or already the
   ship's) so the allocation succeeds, then F650's model-to-table switch, the
   bay index into the table (including the documented read past the last table
   into DATA_006180) and F1669 with a random Q31 station matrix followed by the
   <<10 are printed. Models 0x51..0x5a, bays 0..7, both class bytes and both the
   player's and another ship's index are covered. The out position is 24 bytes:
   x, x sign, y, y sign, z, z sign. */
static unsigned long pads_state = 0x2468ace1UL;
static unsigned long pads_random(void)
{
    pads_state ^= pads_state << 13; pads_state &= 0xffffffffUL;
    pads_state ^= pads_state >> 17;
    pads_state ^= pads_state << 5; pads_state &= 0xffffffffUL;
    return pads_state;
}
static int pads_vectors(void)
{
    static unsigned char station[0x160];
    long position[6];
    long rows[9];
    unsigned long i, j;
    fprintf(stderr, "JJFFE pads-v1: original F650's open-air pad offset (the model table switch, the bay index including the DATA_006180 overrun, F1669 and <<10) on 8,192 synthetic station records; the indoor branch is the fixed (0,0,-1).\n");
    for (i = 0; i < 8192; ++i) {
        unsigned long model = 0x51 + i % 10;
        unsigned long bay = pads_random() % 8;
        unsigned long ship = pads_random() % 12 + 1;
        unsigned long other = ship == 1 ? 2 : 1;
        int open_air = (int)(pads_random() & 1);
        int player = (int)(pads_random() & 1);
        int existing = (int)(pads_random() & 1);
        unsigned long result;
        long flag = 0x5a5a5a5a, pad = 0x5a5a5a5a;
        memset(station, 0, sizeof station);
        for (j = 0; j < 6; ++j) position[j] = 0x5a5a5a5a;
        for (j = 0; j < 9; ++j) rows[j] = (long)(int)pads_random();
        memcpy(station, rows, sizeof rows);
        *(unsigned long *)(station + 0x82) = model;
        station[0x86] = (unsigned char)ship; /* F650's player-event argument */
        *(unsigned short *)(station + 0xce) = (unsigned short)bay;
        for (j = 0; j < 8; ++j) station[0xd0 + j] = (unsigned char)other;
        station[0xd0 + bay] = existing ? (unsigned char)ship : 0;
        station[0x14c] = open_air ? 0x40 : 0x10;
        ref_player_index = player ? ship : 0x7f;
        result = ref_traffic_alloc(station, (long)ship, position, &flag, &pad) & 0xffffffffUL;
        printf("{\"case\":%lu,\"model\":%lu,\"bay\":%lu,\"open_air\":%d,\"player\":%d,\"existing\":%d,\"ship\":%lu,\"rows\":[", i, model, bay, open_air, player, existing, ship);
        for (j = 0; j < 9; ++j) printf("%s%ld", j ? "," : "", rows[j]);
        printf("],\"result\":%ld,\"flag\":%ld,\"pad\":%ld,\"out\":[", (long)result, flag, pad);
        for (j = 0; j < 6; ++j) printf("%s%ld", j ? "," : "", position[j]);
        printf("]}\n");
    }
    if (fflush(stdout)) fail("cannot flush pad vectors");
    return 0;
}


/* Market oracle (--market): the original F619 (price) and F620 (stock), with the game's
   own tables and generator, for random commodities, populations, dangers, supply classes
   and flags (negative variance, illegal) from random generator states. Each case runs the
   price from the state, then the stock from the same state; the results and the states
   after are printed. */
extern long ref_market_price(void *good, long population, long danger, long supply, long flags);
extern long ref_market_stock(void *good, long population, long danger, long supply, long flags);
extern unsigned char ref_market_goods[];
static unsigned long market_state = 0x2545f491UL;
static unsigned long market_random(void)
{
    market_state ^= market_state << 13; market_state &= 0xffffffffUL;
    market_state ^= market_state >> 17;
    market_state ^= market_state << 5; market_state &= 0xffffffffUL;
    return market_state;
}
static int market_vectors(void)
{
    unsigned long i;
    fprintf(stderr, "JJFFE market-v1: original F619 (price) and F620 (stock) with the game's tables and generator on 8,192 random cases; the stock follows the price from the same generator state.\n");
    for (i = 0; i < 8192; ++i) {
        unsigned long good = market_random() % 33, population = market_random() % 10;
        unsigned long danger = market_random() % 8, flags = market_random() & 0xff;
        unsigned long a = market_random(), b = market_random();
        long price, stock;
        unsigned long price_a, price_b, stock_a, stock_b;
        state_set_u32(0x24, a); state_set_u32(0x28, b);
        price = ref_market_price(ref_market_goods + 16 * good, (long)population, (long)danger, (long)(flags & 7), (long)(signed char)flags);
        price_a = state_u32(0x24); price_b = state_u32(0x28);
        state_set_u32(0x24, a); state_set_u32(0x28, b);
        stock = ref_market_stock(ref_market_goods + 16 * good, (long)population, (long)danger, (long)(flags & 7), (long)(signed char)flags);
        stock_a = state_u32(0x24); stock_b = state_u32(0x28);
        printf("{\"good\":%lu,\"population\":%lu,\"danger\":%lu,\"flags\":%lu,\"a\":%lu,\"b\":%lu,\"price\":%ld,\"price_a\":%lu,\"price_b\":%lu,\"stock\":%ld,\"stock_a\":%lu,\"stock_b\":%lu}\n",
               good, population, danger, flags, a, b, price, price_a, price_b, stock, stock_a, stock_b);
    }
    if (fflush(stdout)) fail("cannot flush market vectors");
    return 0;
}


/* Lift scene oracle (--lift): the original stage functions F1069-F1087 (the docking and launch
   lift scenes) and F1086, with the draw call F1070 makes through DATA_007807 replaced by a
   capture, on random stage timeouts and frame times, ships and stations. Prints the objects the
   stage would draw (model, position dwords, orientation, animation words) and the stage state
   and timeout after the call. The expiry frames run F48's event dispatch, which does nothing
   here. */
extern void ref_lift_1069(void);
extern void ref_lift_1074(void);
extern void ref_lift_1075(void);
extern void ref_lift_1076(void);
extern void ref_lift_1078(void);
extern void ref_lift_1081(void);
extern void ref_lift_1082(void);
extern void ref_lift_1083(void);
extern void ref_lift_1084(void);
extern void ref_lift_1085(void);
extern void ref_lift_1087(void);
extern void ref_lift_leave(void *station, long timeout);
extern void (*ref_draw_hook)(void *);
extern unsigned char ref_stage_state;
extern unsigned long ref_stage_timeout;
extern signed char ref_scene_flag;
extern unsigned char *ref_station_object;
static void (*const lift_functions[])(void) = {
    ref_lift_1069,
    ref_lift_1074,
    ref_lift_1075,
    ref_lift_1076,
    ref_lift_1078,
    ref_lift_1081,
    ref_lift_1082,
    ref_lift_1083,
    ref_lift_1084,
    ref_lift_1085,
    ref_lift_1087,
};
static const unsigned char lift_stages[] = {0x0c, 0x0e, 0x10, 0x12, 0x14, 0x16, 0x18, 0x1a, 0x1c, 0x1e, 0x20};
static unsigned char lift_capture_data[4][0x160];
static int lift_captured;
static void lift_capture(void *object)
{
    if (lift_captured < 4) memcpy(lift_capture_data[lift_captured], object, 0x160);
    ++lift_captured;
}
static unsigned long lift_state = 0x6d2b79f5UL;
static unsigned long lift_random(void)
{
    lift_state ^= lift_state << 13; lift_state &= 0xffffffffUL;
    lift_state ^= lift_state >> 17;
    lift_state ^= lift_state << 5; lift_state &= 0xffffffffUL;
    return lift_state;
}
static int lift_vectors(void)
{
    static const unsigned long ships[] = {14, 20, 23, 35, 38, 40, 17, 28};
    static unsigned char player[0x160], station[0x160];
    static const unsigned long frames[] = {0, 1, 994, 1988, 9940, 0x4000, 0x20000, 0x7fffffffUL};
    unsigned long i;
    fprintf(stderr, "JJFFE lift-v1: original lift scene stage functions F1069-F1087 and F1086 with the draw call captured, on 8,192 random stage timeouts, frame times, ships and stations.\n");
    ref_scene_flag = -1;
    ref_draw_hook = lift_capture;
    for (i = 0; i < 8192; ++i) {
        unsigned long which = lift_random() % 12, model = ships[lift_random() % 8], k;
        long timeout; unsigned long frame = frames[lift_random() % 8], tics = lift_random();
        unsigned char stage, flag = lift_random() % 2 ? 0x20 : 0, seat = (unsigned char)(lift_random() % 8);
        unsigned long seed = lift_random();
        memset(player, 0, sizeof player); memset(station, 0, sizeof station);
        *(unsigned long *)(player + 0x82) = model;
        *(unsigned long *)(station + 0x82) = 72;
        *(unsigned long *)(station + 0xa0) = seed;
        station[0x14c] = flag;
        strcpy((char *)station + 0x124, "Daedalus Station");
        state_set_u32(0xc8, (unsigned long)player); state_set_u32(0xcc, (unsigned long)station);
        ref_station_object = station;
        ref_state[0xf4] = seat;
        state_set_u32(0, tics);
        ref_frame_time = frame;
        lift_captured = 0;
        if (which == 11) {
            timeout = (long)(lift_random() % 0x22000) - 0x1000;
            stage = 0x22;
            ref_lift_leave(station, timeout);
            ref_stage_state = stage; ref_stage_timeout = (unsigned long)timeout;
        } else {
            unsigned long span = lift_random() % 4 == 0 ? (frame ? frame + 1 : 1) : 0x20000;
            stage = lift_stages[which];
            timeout = (long)(lift_random() % span);
            ref_stage_state = stage; ref_stage_timeout = (unsigned long)timeout;
            lift_functions[which]();
        }
        printf("{\"stage\":%u,\"timeout\":%ld,\"frame\":%lu,\"ship\":%lu,\"seat\":%u,\"tics\":%lu,\"seed\":%lu,\"flag\":%u,\"after_state\":%u,\"after_timeout\":%ld,\"objects\":[",
               stage, timeout, frame, model, seat, tics, seed, flag, ref_stage_state, (long)ref_stage_timeout);
        for (k = 0; k < (unsigned long)lift_captured && k < 4; ++k) {
            const unsigned char *o = lift_capture_data[k];
            unsigned long j;
            printf("%s{\"model\":%lu,\"word\":%u,\"time_word\":%u,\"seed\":%lu,\"outdoor\":%lu,\"position\":[",
                   k ? "," : "", *(unsigned long *)(o + 0x82), *(unsigned short *)(o + 0x9e), *(unsigned short *)(o + 0x9c),
                   *(unsigned long *)(o + 0xa0), *(unsigned long *)(o + 0xa4));
            for (j = 0; j < 6; ++j) printf("%s%lu", j ? "," : "", *(unsigned long *)(o + 0x26 + 4 * j));
            printf("],\"matrix\":[");
            for (j = 0; j < 9; ++j) printf("%s%ld", j ? "," : "", *(long *)(o + 4 * j));
            printf("],\"name\":\"%s\"}", (const char *)(o + 0x124));
        }
        printf("],\"count\":%d}\n", lift_captured);
    }
    if (fflush(stdout)) fail("cannot flush lift vectors");
    return 0;
}


/* Intro oracle (--intro): the real F961 (model 177/176 and the primary bitmap) and F962 (the
   credit lines) at chosen GameTics. The model draw pointer DATA_007807 and the text draw pointer
   DATA_007644 are captured; the real F1343 expands each credit string, which is printed plain
   (line breaks kept, the in-line font codes replaced by a space). The version line is drawn by
   setting the GUI action to 'V' for one case. The clocks, model thresholds, positions, wrap
   widths and strings are the DOS/JJFFE data that agree; the legal splash is not this oracle's. */
extern void ref_intro_961(void);
extern void ref_intro_962(void);
extern void (*ref_intro_text_hook)(void);
extern unsigned char ref_intro_animation[];
extern char *ref_expand_string(char *dest, long ffcode, void *vars);
extern void ref_sys_init(void);
extern void ref_palette(void *colour);
extern void ref_display_mode(void);
extern void ref_clear_buffer(void);
extern long ref_gui_get_last_action(void);
extern void ref_gui_set_last_action(long action);
#define INTRO_TEXTS 8
static struct {
    long code, colour, x, y, wrap;
    char text[1024];
} intro_text[INTRO_TEXTS];
static int intro_text_count;
static long intro_model, intro_animation;
static int intro_model_count;

static void intro_plain(const unsigned char *src, char *dest)
{
    int at = 0;
    while (*src && at < 1000) {
        unsigned char c = *src;
        if (c == 0x0d) { dest[at++] = '\n'; ++src; }
        else if (c == 0xfe) { src += src[1] && src[2] ? 3 : 1; dest[at++] = ' '; }
        else if (c == 0xfd || c == 0xff) { src += src[1] ? 2 : 1; dest[at++] = ' '; }
        else if (c == 0x1e) { src += src[1] ? 2 : 1; dest[at++] = ' '; }
        else if (c == 0x1f) { src += src[1] && src[2] ? 3 : 1; dest[at++] = ' '; }
        else if (c == 0x03) { ++src; }
        else if (c >= 0x20 && c < 0x7f) { dest[at++] = (char)c; ++src; }
        else { ++src; }
    }
    dest[at] = 0;
}

static long intro_text_draw(long ffcode, void *vars, long colour, long x, long y, long wrap)
{
    if (intro_text_count < INTRO_TEXTS) {
        static char expanded[2048];
        int k = intro_text_count;
        memset(expanded, 0, sizeof expanded);
        ref_expand_string(expanded, ffcode, vars);
        intro_text[k].code = ffcode;
        intro_text[k].colour = colour;
        intro_text[k].x = x;
        intro_text[k].y = y;
        intro_text[k].wrap = wrap;
        intro_plain((const unsigned char *)expanded, intro_text[k].text);
    }
    ++intro_text_count;
    return 1;
}

static void intro_model_draw(void *object)
{
    unsigned char *o = (unsigned char *)object;
    intro_model = *(unsigned long *)(o + 0x82);
    intro_animation = *(unsigned short *)ref_intro_animation;
    ++intro_model_count;
}

static void intro_print_string(const char *s)
{
    putchar('"');
    for (; *s; ++s) {
        unsigned char c = (unsigned char)*s;
        if (c == '"' || c == '\\') printf("\\%c", c);
        else if (c == '\n') printf("\\n");
        else if (c == '\r') printf("\\r");
        else if (c == '\t') printf("\\t");
        else if (c < 0x20 || c >= 0x7f) printf("\\u%04x", c);
        else putchar(c);
    }
    putchar('"');
}

static int intro_vectors(void)
{
    /* The credit windows, in F962's order: the first GameTics and the first tics
       it no longer draws (0 = no upper bound). The Journal Entries line starts
       one tic after 0x6d0954 (the original's `jna`), as the Rust table does. */
    static const unsigned long firsts[] = {
        0x8276e, 0x181534, 0x212f5c, 0x2375e6, 0x2d523c, 0x32a17e,
        0x3bbba6, 0x48a0b4, 0x4ae73e, 0x4d2dc8, 0x588e7a, 0x5ad504,
        0x626ad0, 0x657388, 0x6b84f8, 0x6d0955, 0x77a7d8, 0x77a7d8,
    };
    static const unsigned long ends[] = {
        0xfbd3a, 0x1ee8d2, 0x24fa42, 0x2740cc, 0x34e808, 0x372e92,
        0x3f868c, 0x4d2dc8, 0x4f7452, 0x50f8ae, 0x5d1b8e, 0x5e9fea,
        0x67ba12, 0x693e6e, 0x70d43a, 0x725896, 0, 0,
    };
    static const unsigned long tics[] = {
        0, 497000, 0x795cc, 0x795cc + 1, 0x8276e - 1, 0x8276e, 0x7e7b75,
    };
    unsigned long n;
    fprintf(stderr, "JJFFE intro-v1: the original F961 (model 177/176, animation, primary bitmap 0x42) and F962 (the 18 credit strings with their GameTics windows, positions and wrap widths) at the threshold boundaries and extras; text expanded by F1343; the legal splash is not run.\n");
    ref_sys_init();
    {
        static unsigned char black[3] = {0, 0, 0};
        ref_palette(black);
    }
    ref_display_mode();
    ref_clear_buffer();
    ref_intro_text_hook = (void (*)(void))intro_text_draw;
    ref_draw_hook = intro_model_draw;
    for (n = 0; n < sizeof(tics) / sizeof(tics[0]); ++n) {
        unsigned long at = tics[n], k;
        intro_text_count = 0; intro_model_count = 0;
        *(unsigned long *)ref_state = at;
        ref_gui_set_last_action(0);
        ref_intro_961();
        ref_intro_962();
        printf("{\"tics\":%lu,\"model\":%ld,\"animation\":%ld,\"version\":0,\"texts\":[",
               at, intro_model, intro_animation);
        for (k = 0; k < (unsigned long)intro_text_count && k < INTRO_TEXTS; ++k) {
            printf("%s{\"code\":%ld,\"colour\":%ld,\"x\":%ld,\"y\":%ld,\"wrap\":%ld,\"text\":",
                   k ? "," : "", intro_text[k].code, intro_text[k].colour,
                   intro_text[k].x, intro_text[k].y, intro_text[k].wrap);
            intro_print_string(intro_text[k].text);
            printf("}");
        }
        printf("]}\n");
    }
    for (n = 0; n < sizeof(firsts) / sizeof(firsts[0]); ++n) {
        unsigned long at[4], k, m = 0;
        at[m++] = firsts[n] - 1;
        at[m++] = firsts[n];
        if (ends[n]) { at[m++] = ends[n] - 1; at[m++] = ends[n]; }
        for (k = 0; k < m; ++k) {
            unsigned long j;
            intro_text_count = 0; intro_model_count = 0;
            *(unsigned long *)ref_state = at[k];
            ref_gui_set_last_action(0);
            ref_intro_961();
            ref_intro_962();
            printf("{\"tics\":%lu,\"model\":%ld,\"animation\":%ld,\"version\":0,\"texts\":[",
                   at[k], intro_model, intro_animation);
            for (j = 0; j < (unsigned long)intro_text_count && j < INTRO_TEXTS; ++j) {
                printf("%s{\"code\":%ld,\"colour\":%ld,\"x\":%ld,\"y\":%ld,\"wrap\":%ld,\"text\":",
                       j ? "," : "", intro_text[j].code, intro_text[j].colour,
                       intro_text[j].x, intro_text[j].y, intro_text[j].wrap);
                intro_print_string(intro_text[j].text);
                printf("}");
            }
            printf("]}\n");
        }
    }
    {
        unsigned long at = 0x8276e, k;
        intro_text_count = 0; intro_model_count = 0;
        *(unsigned long *)ref_state = at;
        ref_gui_set_last_action(0x56);
        ref_intro_961();
        ref_intro_962();
        printf("{\"tics\":%lu,\"model\":%ld,\"animation\":%ld,\"version\":1,\"texts\":[",
               at, intro_model, intro_animation);
        for (k = 0; k < (unsigned long)intro_text_count && k < INTRO_TEXTS; ++k) {
            printf("%s{\"code\":%ld,\"colour\":%ld,\"x\":%ld,\"y\":%ld,\"wrap\":%ld,\"text\":",
                   k ? "," : "", intro_text[k].code, intro_text[k].colour,
                   intro_text[k].x, intro_text[k].y, intro_text[k].wrap);
            intro_print_string(intro_text[k].text);
            printf("}");
        }
        printf("]}\n");
    }
    if (fflush(stdout)) fail("cannot flush intro vectors");
    return 0;
}


/* Galaxy sector oracle (--sectors): the original F841 (a sector's stars, with the F861 generator
   and the hand-made sectors), F853 (the star count at several detail levels), F856 (a star's
   position) and F833 (the distance between two positions) on every hand-made sector, random
   sectors and random vectors and points. */
extern long ref_sector_stars(void *vectors, void *info, long x16, long y16, long render);
extern long ref_sector_count(long x, long y, long quality);
extern void ref_star_position(long x16, long y16, void *vector, long *out);
extern long ref_star_distance(long ax, long ay, long az, long bx, long by, long bz);
extern unsigned long ref_special_sectors[];
static unsigned long sector_state = 0x3c6ef372UL;
static unsigned long sector_random(void)
{
    sector_state ^= sector_state << 13; sector_state &= 0xffffffffUL;
    sector_state ^= sector_state >> 17;
    sector_state ^= sector_state << 5; sector_state &= 0xffffffffUL;
    return sector_state;
}
static void sector_case(unsigned long x, unsigned long y)
{
    static unsigned char vectors[64 * 6], info[0x200];
    long n, i;
    memset(vectors, 0, sizeof vectors); memset(info, 0, sizeof info);
    n = ref_sector_stars(vectors, info, (long)(x << 16), (long)(y << 16), 0);
    printf("{\"k\":\"sector\",\"x\":%lu,\"y\":%lu,\"n\":%ld,\"stars\":[", x, y, n);
    for (i = 0; i < n && i < 63; ++i) {
        long j;
        printf("%s{\"v\":[", i ? "," : "");
        for (j = 0; j < 6; ++j) printf("%s%u", j ? "," : "", vectors[6 * i + j]);
        printf("],\"kind\":%u}", (unsigned)(info[0xa6 + 2 * i] | info[0xa7 + 2 * i] << 8));
    }
    printf("]}\n");
}
static int sector_vectors(void)
{
    static const long qualities[] = {0, 0, 0, 1, 5, 15, 16, 20};
    unsigned long i;
    fprintf(stderr, "JJFFE sectors-v1: original F841 (stars of a sector), F853 (star counts), F856 (star positions) and F833 (distances) on every hand-made sector and random sectors, vectors and points.\n");
    for (i = 0; i < 49; ++i) {
        unsigned long key = ref_special_sectors[i];
        sector_case(key & 0xffff, key >> 16);
    }
    for (i = 0; i < 3000; ++i) {
        unsigned long x = i < 2000 ? sector_random() % 0x2000 : 0x1700 + sector_random() % 0x40;
        unsigned long y = i < 2000 ? sector_random() % 0x2000 : 0x1500 + sector_random() % 0x40;
        if (i % 97 == 0) { x = (i % 2) ? 0x2000 + sector_random() % 5 : 0x1fff; y = sector_random() % 0x2000; }
        sector_case(x, y);
    }
    for (i = 0; i < 4000; ++i) {
        unsigned long x = sector_random() % 0x2100, y = sector_random() % 0x2100;
        long quality = qualities[i % 8];
        printf("{\"k\":\"count\",\"x\":%lu,\"y\":%lu,\"quality\":%ld,\"value\":%ld}\n", x, y, quality, ref_sector_count((long)x, (long)y, quality));
    }
    for (i = 0; i < 3000; ++i) {
        unsigned char vector[6];
        long out[3] = {0x55555555, 0x55555555, 0x55555555}, j;
        unsigned long x = sector_random() % 0x2000, y = sector_random() % 0x2000;
        for (j = 0; j < 6; ++j) vector[j] = (unsigned char)sector_random();
        vector[0] = 9;
        ref_star_position((long)(x << 16), (long)(y << 16), vector, out);
        printf("{\"k\":\"position\",\"x\":%lu,\"y\":%lu,\"v\":[%u,%u,%u,%u,%u,%u],\"out\":[%ld,%ld,%ld]}\n", x, y,
               vector[0], vector[1], vector[2], vector[3], vector[4], vector[5], out[0], out[1], out[2]);
    }
    for (i = 0; i < 6000; ++i) {
        long p[6], j;
        long span = i % 5 == 0 ? 0x7fffffffL : (i % 5 == 1 ? 0x10000000L : (i % 5 == 2 ? 0x1000000L : 0x200000L));
        for (j = 0; j < 6; ++j) p[j] = i % 5 == 0 ? (long)sector_random() : (long)(sector_random() % (unsigned long)(span / 2 * 2 + 1)) - span / 2;
        if (i % 7 == 0) for (j = 3; j < 6; ++j) p[j] = p[j - 3] + (long)(sector_random() % 0x4000) - 0x2000;
        printf("{\"k\":\"distance\",\"p\":[%ld,%ld,%ld,%ld,%ld,%ld],\"value\":%ld}\n", p[0], p[1], p[2], p[3], p[4], p[5],
               ref_star_distance(p[0], p[1], p[2], p[3], p[4], p[5]));
    }
    if (fflush(stdout)) fail("cannot flush sector vectors");
    return 0;
}


/* Hyperdrive oracle (--hyperspace): the original F903 (the test and the jump: range, fuel and
   time) for non-player ships (fuel in the object) and for the player (hydrogen or military fuel
   in the hold, the radioactive waste, the cargo used and the "jump possible" flag), on random
   ships, drive classes, distances, fuel and modes. */
extern long ref_hyper(void *ship, long *distance, long *name, long *fuel, long *range, long *time, long *max_range, long mode);
extern unsigned short ref_hydrogen, ref_military_fuel, ref_radioactives, ref_player_flags;
extern unsigned long ref_cargo_used;
static unsigned long hyper_state = 0x1b873593UL;
static unsigned long hyper_random(void)
{
    hyper_state ^= hyper_state << 13; hyper_state &= 0xffffffffUL;
    hyper_state ^= hyper_state >> 17;
    hyper_state ^= hyper_state << 5; hyper_state &= 0xffffffffUL;
    return hyper_state;
}
static int hyper_vectors(void)
{
    static const unsigned long ships[] = {14, 15, 16, 17, 20, 23, 28, 33, 35, 38, 40};
    static unsigned char ship[0x160];
    unsigned long i;
    fprintf(stderr, "JJFFE hyperspace-v1: original F903 (hyperdrive range, fuel and jump time; the test and the jump) on 8,192 random ships, drive classes, distances and fuel, for a non-player ship and the player.\n");
    for (i = 0; i < 8192; ++i) {
        unsigned long model = ships[hyper_random() % 11], player = i % 2, mode = 1 + hyper_random() % 2;
        long class_ = (long)(hyper_random() % 15), distance, name = 0x5a5a5a5aL, fuel = 0x5a5a5a5aL, range = 0x5a5a5a5aL;
        long time = 0x5a5a5a5aL, max_range = 0x5a5a5a5aL, result;
        unsigned long fuel_in = hyper_random() % 3 == 0 ? hyper_random() % 4 : hyper_random() % 120;
        unsigned long flags = hyper_random() & 0xffff;
        long reach;
        unsigned long used_in, radio_in;
        memset(ship, 0, sizeof ship);
        *(unsigned long *)(ship + 0x82) = model;
        ship[0x86] = 5; ship[0xd0] = (unsigned char)class_;
        ref_player_index = player ? 5 : 0x7f;
        ref_hydrogen = 0; ref_military_fuel = 0; radio_in = hyper_random() % 50; ref_radioactives = (unsigned short)radio_in;
        used_in = hyper_random() % 80; ref_cargo_used = used_in; ref_player_flags = (unsigned short)flags;
        if (!player) ship[0x119] = (unsigned char)fuel_in;
        else if (class_ >= 10) ref_military_fuel = (unsigned short)fuel_in;
        else ref_hydrogen = (unsigned short)fuel_in;
        reach = 0;
        { /* a distance around the drive's reach: ask the drive for it */
            long d = 0, n = 0, f = 0, r = 0, t = 0, m = 0;
            ref_hyper(ship, &d, &n, &f, &r, &t, &m, 1);
            reach = m;
        }
        switch (hyper_random() % 6) {
        case 0: distance = 0; break;
        case 1: distance = reach + 1 + (long)(hyper_random() % 100); break;
        case 2: distance = reach; break;
        default: distance = reach > 0 ? 1 + (long)(hyper_random() % (unsigned long)reach) : 1; break;
        }
        ref_player_flags = (unsigned short)flags;
        result = ref_hyper(ship, &distance, &name, &fuel, &range, &time, &max_range, (long)mode);
        printf("{\"model\":%lu,\"class\":%ld,\"player\":%lu,\"mode\":%lu,\"distance_in\":%ld,\"fuel_in\":%lu,\"flags_in\":%lu,\"used_in\":%lu,\"radio_in\":%lu,\"result\":%ld,\"name\":%ld,\"fuel\":%ld,\"range\":%ld,\"time\":%ld,\"max_range\":%ld,\"ship_fuel\":%u,\"hydrogen\":%u,\"military\":%u,\"radioactives\":%u,\"flags\":%u,\"used\":%lu}\n",
               model, class_, player, mode, (long)distance, fuel_in, flags, used_in, radio_in, result, name, fuel, range, time, max_range,
               (unsigned)ship[0x119], (unsigned)ref_hydrogen, (unsigned)ref_military_fuel, (unsigned)ref_radioactives, (unsigned)ref_player_flags, (unsigned long)ref_cargo_used);
    }
    if (fflush(stdout)) fail("cannot flush hyperspace vectors");
    return 0;
}


/* System name oracle (--system-names): the original F830, the name of a system by its code (a hand-made
   star from the string table, the Gateway, or a generated pronounceable name), a string by table index, and
   the "(x,y)" sector form, for every hand-made star and random system codes. */
extern char *ref_system_name(long selector, void *vars, char *dest, unsigned long code);
extern unsigned long ref_special_sectors[];
extern unsigned long ref_special_index[];
static unsigned long name_oracle_state = 0x9e3779b9UL;
static unsigned long name_oracle_random(void)
{
    name_oracle_state ^= name_oracle_state << 13; name_oracle_state &= 0xffffffffUL;
    name_oracle_state ^= name_oracle_state >> 17;
    name_oracle_state ^= name_oracle_state << 5; name_oracle_state &= 0xffffffffUL;
    return name_oracle_state;
}
static void system_name_case(long selector, unsigned long code)
{
    static unsigned char vars[64];
    char dest[512];
    unsigned long i;
    memset(vars, 0, sizeof vars); memset(dest, 0, sizeof dest);
    ref_system_name(selector, vars, dest, code);
    printf("{\"selector\":%ld,\"code\":%lu,\"name\":[", selector, code);
    for (i = 0; dest[i] && i < 200; ++i) printf("%s%u", i ? "," : "", (unsigned char)dest[i]);
    printf("]}\n");
}
static int system_name_vectors(void)
{
    unsigned long i, k;
    fprintf(stderr, "JJFFE system-names-v1: original F830 (the name of a system code, a table string, the sector form) on every hand-made star and random codes.\n");
    for (i = 0; i < 49; ++i) {
        unsigned long key = ref_special_sectors[i];
        for (k = 0; k < ref_special_index[i + 1] - ref_special_index[i]; ++k) system_name_case(0x8400, (key & 0xffff) | ((key >> 16) << 13) | (k << 26));
    }
    system_name_case(0x8400, 0x1aa51717UL);
    for (i = 1; i <= 267; i += 2) system_name_case((long)i, 0);
    for (i = 0; i < 40; ++i) system_name_case(0x1fe, (0x1718UL + i * 37 % 200) | ((0x1524UL + i * 53 % 200) << 13));
    for (i = 0; i < 8000; ++i) {
        unsigned long code = name_oracle_random() & 0xffffffffUL;
        if (i % 3 == 0) code = (code & 0xfc00000) | ((name_oracle_random() % 0x2000)) | ((name_oracle_random() % 0x2000) << 13);
        system_name_case(0x8400 | (long)(name_oracle_random() % 2 ? 0 : 0), code);
    }
    if (fflush(stdout)) fail("cannot flush system name vectors");
    return 0;
}


/* Map star oracle (--map-stars): the real F841 builds a sector's star records and the object's
   star fields, then the real F1772 runs model 0xab (opcode 30 0x801e, F1769) with the point, line and
   label calls recorded: every point, stalk line and label F1769 emits, with the label depth keys. */
static int map_star_vectors(void)
{
#ifndef FFE_COMMAND_PROBE
    fail("--map-stars requires the separate instrumented jjffe-commands.exe");
    return 2;
#else
    static unsigned char normals[32768*8];
    static unsigned long model_queue[65536];
    static unsigned long instance[0x160/4], slots[128*9];
    static unsigned char vectors[64*6], object[0x240];
    static const long identity[9]={2147483647,0,0,0,2147483647,0,0,0,2147483647};
    static const long turned[9]={1518500250,0,-1518500250,0,2147483647,0,1518500250,0,1518500250};
    static const long tipped[9]={2147483647,0,0,0,1518500250,1518500250,0,-1518500250,1518500250};
    static const long both[9]={1859775393,0,-1073741824,-759250125,1518500250,-1315016366,759250125,1518500250,1315016366};
    static const long *const matrices[4]={identity,turned,tipped,both};
    unsigned long *model, i, c, k;
    long n;
    fprintf(stderr,"JJFFE map-stars-v1: the real F841 sector records and the real F1772 over model 0xab (F1769 light points): every point, stalk line and label emitted, with label keys and entries. Not pixels.\n");
    model=ref_model(0xab);
    if(!model) fail("model 0xab is missing");
    for(c=0;c<240;++c) {
        unsigned long x, y, flags=(c&1)?0x20:0, depth, offset[2];
        long out;
        const long *matrix=matrices[(c/2)%4];
        if(c<49) { unsigned long key=ref_special_sectors[c]; x=key&0xffff; y=key>>16; }
        else { x=0x1700+sector_random()%0x40; y=0x1500+sector_random()%0x40; if(c%5==0) { x=sector_random()%0x2000; y=sector_random()%0x2000; } }
        memset(object,0,sizeof object);memset(vectors,0,sizeof vectors);
        n=ref_sector_stars(vectors,object,(long)(x<<16),(long)(y<<16),0);
        *(unsigned short *)(object+0x9e)=(unsigned short)flags;
        memset(instance,0,sizeof(instance));memset(slots,0,sizeof(slots));memset(normals,0,sizeof(normals));
        memset(nested_descriptor,0,sizeof(nested_descriptor));
        memcpy((char *)instance+0x14,matrix,36);
        memcpy((char *)instance+0x9c,matrix,36);
        depth=0x4000+sector_random()%0x40000;
        offset[0]=(unsigned long)((long)(sector_random()%(depth*2))-(long)depth);
        offset[1]=(unsigned long)((long)(sector_random()%(depth*2))-(long)depth);
        instance[4/4]=offset[0]; instance[8/4]=offset[1]; instance[0xc/4]=depth;
        instance[0x148/4]=model[5];
        instance[0xf0/4]=2;
        instance[0x14c/4]=(unsigned long)object;
        instance[0x150/4]=(unsigned long)vectors;
        instance[0x154/4]=model[3];
        ref_vertex_buffer=(long *)slots;
        ref_normal_buffer=normals;
        ref_detail_primary=ref_detail_secondary=0;
        nested_descriptor[0x1c/4]=-0x40000000L;
        memset(model_queue,0,sizeof(model_queue));
        ref_order_base=model_queue;ref_order_active=0;ref_order_state=0;
        ref_order_init(model_queue);
        queue_mode=1;queue_count=0;point_projects=1;
        nested_station=1;nested_count=0;point_line_count=0;label_entry_count=0;
        ref_command_alive=1;
        ref_execute(instance,(void *)model[0]);
        queue_mode=0;nested_station=0;point_projects=0;
        out=(long)ref_command_alive;
        printf("{\"case\":%lu,\"x\":%lu,\"y\":%lu,\"n\":%ld,\"flags\":%lu,\"code\":%lu,\"scale\":%lu,\"position\":[%ld,%ld,%ld],\"matrix\":[",
               c,x,y,n,flags,*(unsigned long *)(object+0xa0),model[5],(long)instance[1],(long)instance[2],(long)instance[3]);
        for(k=0;k<9;++k) printf("%s%ld",k?",":"",matrix[k]);
        printf("],\"types\":[");
        for(k=0;k<(unsigned long)n;++k) printf("%s%u",k?",":"",object[0xa6+2*k]);
        printf("],\"records\":[");
        for(k=0;k<(unsigned long)n*6;++k) printf("%s%u",k?",":"",vectors[k]);
        printf("],\"calls\":[");
        for(i=0;i<point_line_count;++i) {
            if(point_line_kind[i]==1) printf("%s[1,%lu,%ld,%lu,%lu]",i?",":"",point_line_a[i]&0xffff,(long)(short)point_line_b[i],point_line_c[i]&0xff,point_line_d[i]);
            else printf("%s[2,%lu,%lu,%lu,0]",i?",":"",point_line_a[i]&0xffff,point_line_b[i]&0xff,point_line_c[i]&0xff);
        }
        printf("],\"labels\":[");
        for(i=0;i<label_entry_count;++i)
            printf("%s[%lu,%lu,%lu]",i?",":"",label_entries[i][1],label_entries[i][2],label_entries[i][3]);
        printf("],\"keys\":[");
        for(i=0;i<nested_count;++i)
            if(nested_kind[i]==17) printf("%s[%lu,%lu]",i?",":"",nested_value[i][0],nested_value[i][1]);
        printf("],\"alive\":%ld}\n",out);
    }
    if (fflush(stdout)) fail("cannot flush map star vectors");
    return 0;
#endif
}


/* Dot oracles. --particle: the original F1610/F1611 particle on a 320x200 buffer (0xff) at centres
   on and off the view and sizes 0..70 (and the 948 limit); the pixels written are counted, bounded
   and hashed. --dots: the original F1699 (a point command) and F1700/F1701 with a prepared vertex
   slot, then the order queue flushed (F1800/F1801 draw the particles): positions written, and
   those written in the colour at the centre. */
extern void ref_draw_particle(const short *centre,long size,unsigned long colour);
extern long *ref_point(void *instance,long material,long angle,long vertex,long flag);
static unsigned long dot_state=0x2545f491UL;
static unsigned long dot_random(void)
{
    dot_state^=dot_state<<13; dot_state&=0xffffffffUL;
    dot_state^=dot_state>>17;
    dot_state^=dot_state<<5; dot_state&=0xffffffffUL;
    return dot_state;
}
static long dot_range(long low,long high) { return low+(long)(dot_random()%(unsigned long)(high-low+1)); }
static int particle_vectors(void)
{
    static unsigned char buffer[320*200];
    unsigned long n,i,count;
    long minx,miny,maxx,maxy;
    fprintf(stderr,"JJFFE particle-v1: original F1610/F1611 on a 320x200 buffer filled with 0xff, colour 7, centres from -40..360 by -40..200 and sizes 0..70 (plus 947, 948 and 1000).\n");
    ref_framebuffer=buffer;
    for(n=0;n<6000;++n) {
        short centre[2];
        long size=n%25==0?(n%75==0?947:(n%50==0?948:1000)):(n%3==0?dot_range(0,9):dot_range(0,70));
        centre[0]=(short)(n%7==0?dot_range(-40,360):dot_range(0,319));
        centre[1]=(short)(n%11==0?dot_range(-40,200):dot_range(0,157));
        if(n%13==0) { centre[0]=(short)dot_range(0,4); centre[1]=(short)dot_range(0,4); }
        if(n%17==0) { centre[0]=(short)dot_range(315,320); centre[1]=(short)dot_range(153,160); }
        memset(buffer,0xff,sizeof buffer);
        ref_draw_particle(centre,size,7);
        count=0;minx=miny=100000;maxx=maxy=-1;
        for(i=0;i<sizeof buffer;++i) if(buffer[i]!=0xff) {
            long x=(long)(i%320),y=(long)(i/320);
            ++count;
            if(x<minx) minx=x; if(x>maxx) maxx=x; if(y<miny) miny=y; if(y>maxy) maxy=y;
        }
        printf("{\"case\":%lu,\"centre\":[%d,%d],\"size\":%ld,\"written\":%lu,\"bounds\":[%ld,%ld,%ld,%ld],\"pixels_fnv1a64\":\"%016llx\"}\n",
               n,centre[0],centre[1],size,count,count?minx:0,count?miny:0,count?maxx:0,count?maxy:0,
               (unsigned long long)fingerprint(buffer,sizeof buffer));
    }
    if (fflush(stdout)) fail("cannot flush particle vectors");
    return 0;
}
static int dot_vectors(void)
{
    static unsigned char buffer[320*200];
    static unsigned long instance[0x160/4],slots[128*9],globals[0x120/4];
    unsigned long n,i,count,centre_count;
    long minx,miny,maxx,maxy;
    fprintf(stderr,"JJFFE dots-v1: original F1699/F1700/F1701 for unlit points over a prepared vertex slot, the order queue flushed through F1789 (F1800/F1801 particles); pixels written and the pixels in the centre pixel's value, hashed by position.\n");
    ref_framebuffer=buffer;
    for(n=0;n<6000;++n) {
        long material,angle,scale,x,y,depth,vx,vy;
        unsigned char colour[3];
        unsigned long positions=UINT64_C(14695981039346656037)&0xffffffffUL,centre_positions=0;
        unsigned char centre_value;
        material=0x2000|(dot_range(0,15)<<8)|(dot_range(0,15)<<4)|dot_range(0,15);
        if(n%9==0) material|=0x1000;
        scale=dot_range(6,11);
        angle=dot_range(1,11)<<(n%5==0?0:2);
        {
            long radius=((angle&0x7f)<<9),span;
            radius=(short)radius;
            radius=scale<8?(long)((unsigned long)radius>>(8-scale)):radius<<(scale-8);
            if(radius<1) radius=1;
            span=radius*8;
            /* Mostly the far path (depth 8 to 300 times the radius); some the near path and the cut-offs. */
            if(n%10==0) depth=dot_range(radius>0x40?radius:0x40,span);
            else if(n%37==0) depth=dot_range(0,0x80);
            else depth=span+dot_range(0,span*(n%3==0?40:6));
            if(depth<0) depth=0;
        }
        x=n%7==0?dot_range(-30,350):dot_range(0,319);
        y=n%11==0?dot_range(-30,190):dot_range(0,157);
        colour[0]=(unsigned char)dot_range(0,15);colour[1]=(unsigned char)dot_range(0,15);colour[2]=(unsigned char)dot_range(0,15);
        memset(instance,0,sizeof instance);memset(slots,0,sizeof slots);memset(globals,0,sizeof globals);
        instance[0xf0/4]=2;
        instance[0x148/4]=(unsigned long)scale;
        instance[0x14c/4]=(unsigned long)globals;
        memcpy((char *)instance+0xf4,colour,3);
        slots[0]=((unsigned long)x&0xffff)|((unsigned long)y<<16);
        vx=dot_range(-50000,50000);vy=dot_range(-50000,50000);
        slots[1]=(unsigned long)vx;
        slots[2]=(unsigned long)vy;
        slots[3]=(unsigned long)depth;
        slots[5]=2;
        ref_vertex_buffer=(long *)slots;
        order_init();
        memset(buffer,0xff,sizeof buffer);
        ref_point(instance,material,angle,0,0);
        ref_order_group(0,0);
        ref_order_flush();
        count=0;centre_count=0;minx=miny=100000;maxx=maxy=-1;
        centre_value=(x>=0&&x<320&&y>=0&&y<200)?buffer[y*320+x]:0xff;
        for(i=0;i<sizeof buffer;++i) if(buffer[i]!=0xff) {
            long px=(long)(i%320),py=(long)(i/320);
            ++count;
            positions=(positions^(unsigned long)i)*16777619UL;
            if(buffer[i]==centre_value) { ++centre_count; centre_positions=(centre_positions^(unsigned long)i)*16777619UL; }
            if(px<minx) minx=px; if(px>maxx) maxx=px; if(py<miny) miny=py; if(py>maxy) maxy=py;
        }
        printf("{\"case\":%lu,\"material\":%ld,\"angle\":%ld,\"scale\":%ld,\"screen\":[%ld,%ld],\"view\":[%ld,%ld],\"depth\":%ld,\"instance\":[%u,%u,%u],\"written\":%lu,\"bounds\":[%ld,%ld,%ld,%ld],\"positions\":%lu,\"centre\":%u,\"centre_written\":%lu,\"centre_positions\":%lu}\n",
               n,material,angle,scale,x,y,vx,vy,depth,colour[0],colour[1],colour[2],count,
               count?minx:0,count?miny:0,count?maxx:0,count?maxy:0,positions&0xffffffffUL,(unsigned)centre_value,centre_count,centre_positions&0xffffffffUL);
    }
    if (fflush(stdout)) fail("cannot flush dot vectors");
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
    if (argc == 2 && (!strcmp(argv[1], "--sysgen") || !strcmp(argv[1], "--worldgen"))) {
        /* Uninitialised stack in F877 holds addresses: fix them so traces repeat. */
        int mode = personality(0xffffffff);
        if (mode != -1 && !(mode & ADDR_NO_RANDOMIZE)) {
            personality(mode | ADDR_NO_RANDOMIZE);
            execv("/proc/self/exe", argv);
        }
    }
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
    if (argc == 2 && !strcmp(argv[1], "--span-raster")) return span_raster_vectors();
    if (argc == 2 && !strcmp(argv[1], "--thrust-limits")) return thrust_limit_vectors();
    if (argc == 2 && !strcmp(argv[1], "--accelerate")) return accelerate_vectors();
    if (argc == 2 && !strcmp(argv[1], "--gravity")) return gravity_vectors();
    if (argc == 2 && !strcmp(argv[1], "--frames")) return frames_vectors();
    if (argc == 2 && !strcmp(argv[1], "--terrain-mid")) return terrain_mid_vectors();
    if (argc == 2 && !strcmp(argv[1], "--terrain")) return terrain_vectors();
    if (argc == 2 && !strcmp(argv[1], "--surface-draw")) return surface_draw_vectors(-1);
    if (argc == 3 && !strcmp(argv[1], "--surface-draw")) return surface_draw_vectors(atoi(argv[2]));
    if (argc == 2 && !strcmp(argv[1], "--planet")) return planet_vectors();
    if (argc == 2 && !strcmp(argv[1], "--names")) return name_vectors();
    if (argc == 2 && !strcmp(argv[1], "--sysgen")) return sysgen_vectors();
    if (argc == 2 && !strcmp(argv[1], "--worldgen")) return worldgen_vectors();
    if (argc == 2 && !strcmp(argv[1], "--galaxy")) return galaxy_vectors();
    if (argc == 2 && !strcmp(argv[1], "--collide")) return collide_vectors();
    if (argc == 2 && !strcmp(argv[1], "--world-step")) return world_step_vectors();
    if (argc == 2 && !strcmp(argv[1], "--world-tick")) return world_tick_vectors();
    if (argc == 2 && !strcmp(argv[1], "--orient")) return orient_vectors();
    if (argc == 2 && !strcmp(argv[1], "--player-orient")) return player_orient_vectors();
    if (argc == 2 && !strcmp(argv[1], "--advance")) return advance_vectors();
    if (argc == 2 && !strcmp(argv[1], "--flight-step")) return flight_step_vectors();
    if (argc == 2 && !strcmp(argv[1], "--steer")) return steer_vectors();
    if (argc == 3 && !strcmp(argv[1], "--texture-queue")) return texture_queue_vector(atoi(argv[2]));
    if (argc == 3 && !strcmp(argv[1], "--child-model")) return child_model_vector(atoi(argv[2]),2000);
    if (argc == 4 && !strcmp(argv[1], "--child-model")) return child_model_vector(atoi(argv[2]),strtol(argv[3],0,0));
    if (argc == 2 && !strcmp(argv[1], "--thruster-cases")) return thruster_case_vectors();
    if (argc == 2 && !strcmp(argv[1], "--base-transform")) return base_transform_vectors();
    if (argc == 2 && !strcmp(argv[1], "--animation")) return animation_vectors();
    if (argc == 2 && !strcmp(argv[1], "--traffic")) return traffic_vectors();
    if (argc == 2 && !strcmp(argv[1], "--pads")) return pads_vectors();
    if (argc == 2 && !strcmp(argv[1], "--market")) return market_vectors();
    if (argc == 2 && !strcmp(argv[1], "--lift")) return lift_vectors();
    if (argc == 2 && !strcmp(argv[1], "--intro")) return intro_vectors();
    if (argc == 2 && !strcmp(argv[1], "--sectors")) return sector_vectors();
    if (argc == 2 && !strcmp(argv[1], "--hyperspace")) return hyper_vectors();
    if (argc == 2 && !strcmp(argv[1], "--system-names")) return system_name_vectors();
    if (argc == 2 && !strcmp(argv[1], "--map-stars")) return map_star_vectors();
    if (argc == 2 && !strcmp(argv[1], "--particle")) return particle_vectors();
    if (argc == 2 && !strcmp(argv[1], "--dots")) return dot_vectors();
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
    if (argc >= 4 && !strcmp(argv[1], "--game-frame")) {
        int k;
        game_frame_mode = 1; limit = strtoul(argv[2], 0, 0);
        for (k = 3; k < argc && game_key_count < 16; ++k) game_keys[game_key_count++] = strtoul(argv[k], 0, 0);
        fprintf(stderr, "JJFFE game-frame-v1: the real game from the new game, scripted keys, final 320x200 frame and palette in game-frame.bin.\n");
        asmmain(0, NULL);
        return 2;
    }
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
