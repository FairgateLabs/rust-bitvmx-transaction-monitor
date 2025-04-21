use bitcoin::key::rand;
use rand::Rng;

pub fn generate_random_string() -> String {
    let mut rng = rand::thread_rng();
    (0..10).map(|_| rng.gen_range('a'..='z')).collect()
}

pub fn clear_output() {
    let _ = std::fs::remove_dir_all("test_outputs");
}
