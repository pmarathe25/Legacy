@group(0) @binding(0) var<storage, read> input: array<u64>;
@group(0) @binding(1) var<storage, read_write> output: array<vec2<u64>>;
fn add_sat(a: i64, b: i64) -> i64 {
    let v = a + b;
    let overflow = ((a ^ v) & (b ^ v)) < 0li;
    let limit = select(bitcast<i64>(0x8000000000000000lu), 0x7fffffffffffffffli, a >= 0li);
    return select(v, limit, overflow);
}
fn sub_sat(a: i64, b: i64) -> i64 {
    let v = a - b;
    let overflow = ((a ^ b) & (a ^ v)) < 0li;
    let limit = select(bitcast<i64>(0x8000000000000000lu), 0x7fffffffffffffffli, a >= 0li);
    return select(v, limit, overflow);
}
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) global: vec3<u32>) {
    let p = global.x;
    let people = u32(input[0]);
    if p >= people { return; }
    let choices = u32(input[1]);
    let rows = people * choices;
    let current = input[2u + 7u * rows + p];
    var have = false;
    var have_current = false;
    var best_id = 0xfffffffffffffffflu;
    var best_score = bitcast<i64>(0x8000000000000000lu);
    var current_score = 0li;
    for (var c = 0u; c < choices; c += 1u) {
        let r = c * people + p;
        let id = input[2u + r];
        let need = bitcast<i64>(input[2u + rows + r]);
        let goal = bitcast<i64>(input[2u + 2u * rows + r]);
        let value = bitcast<i64>(input[2u + 3u * rows + r]);
        let effort = bitcast<i64>(input[2u + 4u * rows + r]);
        let risk = bitcast<i64>(input[2u + 5u * rows + r]);
        let score = sub_sat(sub_sat(add_sat(add_sat(need, goal), value), effort), risk);
        if id == current { have_current = true; current_score = score; }
        if !have || score > best_score || (score == best_score && id < best_id) {
            have = true; best_id = id; best_score = score;
        }
    }
    if have && have_current && best_score <= add_sat(current_score, 5li) {
        best_id = current; best_score = current_score;
    }
    output[u32(p)] = vec2<u64>(best_id, bitcast<u64>(best_score));
}
