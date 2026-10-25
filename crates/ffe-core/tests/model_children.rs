use ffe_core::model::{VertexContext, IDENTITY};
use ffe_core::model_commands::VisibilityContext;
use ffe_core::model_execute::{
    child_culled, execute_live, execute_scene, ChildEnvironment, ChildModel, ChildModels,
    ChildOutcome, InstanceContext, Primitive, QueueEvent,
};
use ffe_core::model_instance::Vertices;
use ffe_core::model_math::{frame_rows, shift_down, transform_rows};
use ffe_core::model_normals::Normal;

fn bytes(words: &[u16]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

const VISIBLE_NORMAL: Normal = Normal {
    culled: false,
    colour: [0; 3],
};
static VISIBLE: [Normal; 16] = [VISIBLE_NORMAL; 16];

fn visibility() -> VisibilityContext<'static> {
    VisibilityContext {
        depth: 1000,
        scale: 0,
        detail: [0, 0],
        normals: &VISIBLE,
    }
}

/// Model 1: one literal pair and a triangle over external vertex -1 and its own pair.
struct Models {
    child: Vec<u8>,
    vertices: Vec<u8>,
}

impl ChildModels for Models {
    fn model(&self, index: u16) -> Result<ChildModel<'_>, String> {
        if index != 1 {
            return Err(format!("no model {index}"));
        }
        Ok(ChildModel {
            commands: &self.child,
            vertices: &self.vertices,
            vertex_count: 2,
            normals: &[],
            normal_count: 2,
            scale: 0,
            scale_offset: 0,
            radius: 10,
        })
    }
}

fn models(child: &[u16]) -> Models {
    Models {
        child: bytes(child),
        vertices: vec![0, 0, 10, 0, 0, 0],
    }
}

const TRIANGLE: [u16; 5] = [3, 0x0123, 0xff00, 0x0101, 0];
// Parent: child model 1 at vertex 0, frame mode 0, then return.
const PARENT: [u16; 3] = [14 | 1 << 5, 0x0000, 0];
const PARENT_VERTICES: [u8; 6] = [0, 0, 0, 0, 100, 0];

fn environment(models: &Models, arena: u32) -> ChildEnvironment<'_> {
    ChildEnvironment {
        models,
        shades: std::array::from_fn(|i| [i as u8; 3]),
        light: [0x1000, 0, 0],
        arena,
    }
}

fn instance(z: i32) -> InstanceContext {
    InstanceContext {
        position: [0, 0, z],
        generation: 2,
        ..Default::default()
    }
}

#[test]
fn a_failed_child_subtree_does_not_discard_its_siblings() {
    // Model 1 calls model 2 with frame mode 6, copying its own (undefined)
    // parent basis: the original reads stale stack, the port reports the call
    // as failed. The parent's second child (the triangle) must still run.
    struct Partial {
        failing: Vec<u8>,
        triangle: Vec<u8>,
        vertices: Vec<u8>,
    }
    impl ChildModels for Partial {
        fn model(&self, index: u16) -> Result<ChildModel<'_>, String> {
            let (commands, radius) = match index {
                1 => (&self.failing, 10),
                2 => (&self.triangle, 10),
                _ => return Err(format!("no model {index}")),
            };
            Ok(ChildModel {
                commands,
                vertices: &self.vertices,
                vertex_count: 2,
                normals: &[],
                normal_count: 2,
                scale: 0,
                scale_offset: 0,
                radius,
            })
        }
    }
    // Model 1: child model 2 with frame mode 6 (0x0600), then return.
    let failing = bytes(&[14 | 2 << 5, 0x0600, 0]);
    let triangle = bytes(&TRIANGLE);
    let models = Partial {
        failing,
        triangle,
        vertices: vec![0, 0, 10, 0, 0, 0],
    };
    let parent = bytes(&[14 | 1 << 5, 0x0000, 14 | 2 << 5, 0x0000, 0]);
    let mut live = Vertices::new(&PARENT_VERTICES, 2).unwrap();
    let environment = ChildEnvironment {
        models: &models,
        shades: std::array::from_fn(|i| [i as u8; 3]),
        light: [0x1000, 0, 0],
        arena: 0,
    };
    let result = execute_scene(
        &parent,
        &mut live,
        &mut VertexContext::default(),
        &visibility(),
        None,
        instance(1000),
        64,
        &environment,
    )
    .unwrap();
    assert_eq!(result.children.len(), 2);
    let ChildOutcome::Executed(first) = &result.children[0].outcome else {
        panic!("model 1 did not run: {:?}", result.children[0].outcome);
    };
    assert!(
        matches!(
            first.children[0].outcome,
            ChildOutcome::Failed(ref error) if error.contains("copies the parent basis")
        ),
        "the nested call should be reported, not fatal: {:?}",
        first.children[0].outcome
    );
    let ChildOutcome::Executed(second) = &result.children[1].outcome else {
        panic!(
            "the sibling child did not run: {:?}",
            result.children[1].outcome
        );
    };
    assert_eq!(second.emissions.len(), 1);
    // The sibling's triangle still joined the parent's ordered stream.
    assert!(matches!(
        &result.ordered[..],
        [Primitive::Face(face)] if face.face.vertices == [0xff, 0, 1]
    ));
}

#[test]
fn child_runs_with_external_vertices_and_joins_the_parent_queue() {
    let models = models(&TRIANGLE);
    let parent = bytes(&PARENT);
    let mut live = Vertices::new(&PARENT_VERTICES, 2).unwrap();
    let mut context = VertexContext::default();
    context.locals[0] = 41;
    let result = execute_scene(
        &parent,
        &mut live,
        &mut context,
        &visibility(),
        None,
        instance(1000),
        64,
        &environment(&models, 0),
    )
    .unwrap();
    let parent_view = live.slots()[0].view;
    assert_eq!(result.children.len(), 1);
    let run = &result.children[0];
    assert_eq!((run.offset, run.model, run.radius), (0, 1, 10));
    let ChildOutcome::Executed(child) = &run.outcome else {
        panic!("child not executed: {:?}", run.outcome);
    };
    assert_eq!(child.emissions.len(), 1);
    assert_eq!(child.emissions[0].face.vertices, [0xff, 0, 1]);
    assert!(child.queue.is_empty() && child.ordered.is_empty());
    let setup = run.setup.as_ref().unwrap();
    assert_eq!(setup.instance.position, parent_view);
    assert_eq!(setup.instance.generation, 2);
    assert_eq!(setup.externals.sources[0].unwrap().view, parent_view);
    assert!(setup.externals.sources[1..].iter().all(Option::is_none));
    assert_eq!(setup.context.locals[0], 42);
    assert_eq!(setup.context.matrix, IDENTITY);
    // Shade bytes fill locals 12..17; locals 3..11 are undefined in the child.
    assert_eq!(setup.context.locals[12], i32::from_le_bytes([0, 0, 0, 1]));
    assert_eq!(setup.context.undefined_locals, 0xff8);
    assert!(setup.observer.is_some());
    // The child's face is queued in the parent's stream, keyed by the external
    // vertex, whose view is the parent's.
    assert_eq!(result.queue.len(), 1);
    assert!(matches!(result.queue[0], QueueEvent::Face(_)));
    assert!(
        matches!(&result.ordered[..], [Primitive::Face(face)] if face.face.vertices == [0xff, 0, 1])
    );
    assert_eq!(
        result.queue[0],
        QueueEvent::Face(ffe_core::model_depth::key(parent_view, 0))
    );

    // execute_live records the call only.
    let mut live = Vertices::new(&PARENT_VERTICES, 2).unwrap();
    let plain = execute_live(
        &parent,
        &mut live,
        &mut VertexContext::default(),
        &visibility(),
        None,
        instance(1000),
        64,
    )
    .unwrap();
    assert!(plain.children.is_empty() && plain.ordered.is_empty());
    assert_eq!(plain.submodels.len(), 1);
}

#[test]
fn culled_and_roomless_children_do_not_run() {
    let models = models(&TRIANGLE);
    let parent = bytes(&PARENT);
    let mut live = Vertices::new(&PARENT_VERTICES, 2).unwrap();
    let result = execute_scene(
        &parent,
        &mut live,
        &mut VertexContext::default(),
        &visibility(),
        None,
        instance(-500),
        64,
        &environment(&models, 0),
    )
    .unwrap();
    assert_eq!(result.children[0].outcome, ChildOutcome::Culled);
    assert!(result.children[0].setup.is_none() && result.ordered.is_empty());

    // 12 + (6 + 2) * 36 + 2 * 8 = 316 bytes do not fit.
    let mut live = Vertices::new(&PARENT_VERTICES, 2).unwrap();
    let result = execute_scene(
        &parent,
        &mut live,
        &mut VertexContext::default(),
        &visibility(),
        None,
        instance(1000),
        64,
        &environment(&models, 0x8000 - 315),
    )
    .unwrap();
    assert_eq!(result.children[0].outcome, ChildOutcome::NoRoom);
    assert!(result.ordered.is_empty());

    assert!(child_culled([0, 0, 40], 10));
    assert!(!child_culled([0, 0, 54], 10));
    assert!(child_culled([200, 0, 100], 10));
    assert!(!child_culled([-120, 0, 100], 10));
}

#[test]
fn external_references_need_a_parent_and_errors_are_transactional() {
    // Top level: the child program alone cannot resolve vertex -1.
    let child = bytes(&TRIANGLE);
    let vertices = [0u8, 0, 10, 0, 0, 0];
    let mut live = Vertices::new(&vertices, 2).unwrap();
    let mut context = VertexContext::default();
    let before = (context.clone(), live.slots());
    let error = execute_live(
        &child,
        &mut live,
        &mut context,
        &visibility(),
        None,
        instance(1000),
        64,
    )
    .unwrap_err();
    assert!(error.contains("needs a parent instance"), "{error}");
    assert_eq!((context, live.slots()), before);

    // Vertex -2 has no pointer in a call without extra vertices. A child that
    // cannot run is reported in its outcome and leaves the rest of the scene
    // intact, so the parent run itself succeeds.
    let models = models(&[3, 0x0123, 0xfe00, 0x0101, 0]);
    let parent = bytes(&PARENT);
    let mut live = Vertices::new(&PARENT_VERTICES, 2).unwrap();
    let mut context = VertexContext::default();
    let before = context.clone();
    let result = execute_scene(
        &parent,
        &mut live,
        &mut context,
        &visibility(),
        None,
        instance(1000),
        64,
        &environment(&models, 0),
    )
    .unwrap();
    let ChildOutcome::Failed(error) = &result.children[0].outcome else {
        panic!("the child should fail: {:?}", result.children[0].outcome);
    };
    assert!(error.contains("no parent vertex pointer"), "{error}");
    assert_eq!(context, before);
}

#[test]
fn child_locals_follow_the_original_instance_layout() {
    let run = |child: &[u16]| {
        let models = models(child);
        let mut live = Vertices::new(&PARENT_VERTICES, 2).unwrap();
        execute_scene(
            &bytes(&PARENT),
            &mut live,
            &mut VertexContext::default(),
            &visibility(),
            None,
            instance(1000),
            64,
            &environment(&models, 0),
        )
    };
    // Reading local 3 before writing it: stack data in the original. The child
    // is reported failed and the parent run succeeds.
    let result = run(&[13, 0x00c3, 0]).unwrap();
    let ChildOutcome::Failed(error) = &result.children[0].outcome else {
        panic!("the child should fail: {:?}", result.children[0].outcome);
    };
    assert!(error.contains("Local 3 is uninitialised"), "{error}");
    // Writing local 7 would overwrite the +0x70 external pointer.
    let result = run(&[13 | 7 << 8, 0x0000, 0]).unwrap();
    let ChildOutcome::Failed(error) = &result.children[0].outcome else {
        panic!("the child should fail: {:?}", result.children[0].outcome);
    };
    assert!(error.contains("external vertex pointer"), "{error}");
    // Writing local 3 defines it; locals 0..2 and 12..17 are defined from the start.
    let result = run(&[13 | 3 << 8, 0x0005, 13 | 4 << 8, 0xc3cc, 0]).unwrap();
    let ChildOutcome::Executed(child) = &result.children[0].outcome else {
        panic!()
    };
    assert_eq!(child.commands, [0, 4, 8]);
    // Reading the undefined basis (rotation, frame restore) is refused.
    let result = run(&[30 | 0x8000 | 1 << 5, 0]).unwrap();
    let ChildOutcome::Failed(error) = &result.children[0].outcome else {
        panic!("the child should fail: {:?}", result.children[0].outcome);
    };
    assert!(error.contains("basis"), "{error}");
}

#[test]
fn frame_helpers_match_f1739_f1475_and_f1670() {
    let source = [1, 2, 3, 4, 5, 6, 7, 8, 9];
    assert_eq!(frame_rows(&source, 0, 0).unwrap(), (source, 0));
    // Mode 3 reverses rows (odd: flip); 0x2000 negates source row 0 (flip again).
    assert_eq!(
        frame_rows(&source, 0, 0x2300).unwrap(),
        ([7, 8, 9, 4, 5, 6, -1, -2, -3], 0)
    );
    assert_eq!(
        frame_rows(&source, 0, 0x0100).unwrap().0,
        [7, 8, 9, 1, 2, 3, 4, 5, 6]
    );
    assert!(frame_rows(&source, 0, 0x0700).is_err());
    assert_eq!(shift_down([0x3fff, -5, 0]), ([0x3fff, -5, 0], 0));
    assert_eq!(shift_down([0, -0x8000, 0x10]), ([0, -0x2000, 4], 2));
    assert_eq!(
        transform_rows([0x4000_0000, 0, 0], &IDENTITY),
        [0x3fff_ffff, 0, 0]
    );
}
