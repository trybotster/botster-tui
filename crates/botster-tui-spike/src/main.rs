fn main() {
    let proof = botster_tui_spike::run_foundation_proof();

    println!("foundation={}", proof.foundation);
    println!("frame={}x{}", proof.frame_width, proof.frame_height);
    println!("hit_regions={}", proof.hit_regions);
    println!("semantic_actions={}", proof.semantic_actions);
    println!("terminal_forwarded={}", proof.terminal_forwarded);
    println!("redraws={}", proof.redraws);
}
