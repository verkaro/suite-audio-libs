use super::*;

#[test]
fn true_stereo_folds_as_one_signal_hears_it() {
    let c = vec![vec![1.0, 0.0], vec![0.5, 0.0], vec![0.25, 0.0], vec![2.0, 0.0]];
    assert_eq!(fold(c).unwrap(), vec![vec![1.25, 0.0], vec![2.5, 0.0]]);
    assert_eq!(fold(vec![vec![1.0]]).unwrap(), vec![vec![1.0]]);
    assert!(fold(vec![vec![1.0]; 3]).is_none());
}

#[test]
fn unit_energy_is_unit_energy() {
    let c = vec![vec![0.5, 0.5, 0.0, 0.1], vec![0.2, -0.3, 0.4, 0.0]];
    let g = unit_energy(&c);
    let e: f64 = c.iter().flatten().map(|v| ((v * g) as f64).powi(2)).sum::<f64>() / 2.0;
    assert!((e - 1.0).abs() < 1e-6, "{e}");
    assert_eq!(unit_energy(&[vec![0.0; 4]]), 1.0);
}

#[test]
fn echoes_leave_out_the_direct_sound_and_keep_the_rest_in_time() {
    let rate = 48_000;
    // Silence, a direct sound at frame 100, an echo 10 ms after it, a quieter one at 30 ms.
    let mut ir = vec![0.0f32; 4000];
    ir[100] = 1.0;
    ir[101] = 0.5;
    ir[100 + 480] = 0.3;
    ir[100 + 1440] = -0.2;
    let c = vec![ir];
    assert_eq!(onset(&c), 100);
    let e = echoes(&c, rate);
    assert_eq!(e[0].len(), 3900, "from the onset");
    assert!(e[0][..120].iter().all(|v| *v == 0.0), "the direct sound gone");
    assert_eq!((e[0][480], e[0][1440]), (0.3, -0.2), "the echoes where they were, as they were");
    // In the millisecond after the gap, faded in.
    let mut ramp = vec![0.0f32; 400];
    ramp[1] = 1.0;
    ramp[120..200].fill(1.0);
    let f = echoes(&[ramp], rate);
    assert!(
        f[0][120] == 0.0 && f[0][144] > 0.4 && f[0][144] < 0.6 && f[0][168] == 1.0,
        "{:?}",
        &f[0][118..170]
    );
}
