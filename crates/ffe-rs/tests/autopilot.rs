use ffe_rs::flight::{Flight,Mode};
use ffe_rs::original_flight::{OriginalFlight,Keys,UNITS_PER_FLIGHT_UNIT};

#[test]
fn source_controls_fly_and_brake_without_keyboard_speed_or_direct_turns() {
    let mut model=OriginalFlight::from_flight(&Flight { mode:Mode::Flight, system_flight:true,
        axes:Some([[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]]),position:[0.;3],..Flight::default() },None);
    let target=[0,0,1_000_000];
    let start=model.absolute_position();
    for _ in 0..5000 {
        let guidance=model.guidance(target,[0;3],0);
        model.apply_guidance(guidance);
        model.step(0.02,&Keys::default()).unwrap();
    }
    let at=model.absolute_position();
    assert!(at[2]>start[2]+500_000,"source thrust did not advance: {at:?}");
    assert!((at[2]-target[2]).abs()<100_000,"failed to brake near target: {at:?}");
    assert!(model.player().motion.velocity[2].abs()<100,"not braking: {:?}",model.player().motion.velocity);
    model.clear_guidance();
    assert_eq!(model.player().mode,0);
    assert!(model.player().set_speed>=0);
}

#[test]
fn installing_guidance_does_not_snap_orientation_or_position() {
    let mut model=OriginalFlight::from_flight(&Flight { mode:Mode::Flight, system_flight:true,
        axes:Some([[1.,0.,0.],[0.,1.,0.],[0.,0.,1.]]),position:[0.;3],..Flight::default() },None);
    let before=*model.player();
    let command=model.guidance([1_000_000,0,0],[0;3],0);
    model.apply_guidance(command);
    assert_eq!(model.player().motion.position,before.motion.position);
    assert_eq!(model.player().motion.orientation,before.motion.orientation);
    model.step(0.02,&Keys::default()).unwrap();
    assert_ne!(model.player().motion.orientation,before.motion.orientation);
    let mut state=Flight::default(); model.apply(&mut state);
    for (a,b) in state.position.into_iter().zip(model.absolute_position()) {
        assert!((a*UNITS_PER_FLIGHT_UNIT-b as f64).abs()<=1.0);
    }
}
