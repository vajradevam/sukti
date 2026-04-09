//! Data-driven conformance suite for the Sūkti language.
//!
//! Every entry is a real Sūkti program executed through `pipeline::run_source`.
//! `expected` is a substring that must appear on stdout. Cases are grouped by
//! language feature via their name prefixes. The suite is intentionally large:
//! it is a regression net for the whole Tier-0 interpreter.

use sukti::pipeline::{self, Options};

type Case = (&'static str, &'static str, &'static str);

static CASES: &[Case] = &[
    // ---- lit
    ("lit_int_zero", "vada(0);", "0"),
    ("lit_int_one", "vada(1);", "1"),
    ("lit_int_ascii", "vada(42);", "42"),
    ("lit_int_mixed", "vada(123 + 456);", "579"),
    ("lit_int_underscore", "vada(1_234 + 5);", "1239"),
    ("lit_int_ascii_underscore", "vada(1_000 + 24);", "1024"),
    (
        "lit_bigint_mul",
        "vada(12345678901234567890 * 9999999999);",
        "123456788999999999998765432110",
    ),
    (
        "lit_bigint_pow",
        "vada(99 ** 30);",
        "739700373388280422730015092316714942252676262352676444347001",
    ),
    (
        "lit_bigint_add",
        "vada(1000000000000000000000000000000 + 100000000000000000000);",
        "1000000000100000000000000000000",
    ),
    (
        "lit_bigint_sub",
        "vada(10000000000000000000000000 - 1);",
        "9999999999999999999999999",
    ),
    ("lit_neg_basic", "vada(-5);", "-5"),
    ("lit_neg_add", "vada(-5 + 2);", "-3"),
    ("lit_neg_paren", "vada(2 * (-3));", "-6"),
    ("lit_double_neg", "vada(-(-7));", "7"),
    ("lit_precedence_mul", "vada(2 + 3 * 4);", "14"),
    ("lit_precedence_paren", "vada((2 + 3) * 4);", "20"),
    ("lit_precedence_div", "vada(1 + 8 / 4);", "3"),
    ("lit_precedence_mod", "vada(2 * 7 % 4);", "2"),
    ("lit_precedence_pow", "vada(2 + 3 ** 2);", "11"),
    ("lit_pow_assoc", "vada(2 ** 3 ** 2);", "512"),
    ("lit_div_exact", "vada(20 / 4);", "5"),
    ("lit_div_trunc", "vada(7 / 2);", "3"),
    ("lit_mod", "vada(17 % 5);", "2"),
    ("lit_mod_neg", "vada((-17) % 5);", "-2"),
    ("lit_float_lit", "vada(1.5);", "1.5"),
    ("lit_float_add", "vada(1.5 + 2.25);", "3.75"),
    ("lit_float_sub", "vada(5.5 - 2.5);", "3"),
    ("lit_float_mul", "vada(2.5 * 4);", "10"),
    ("lit_float_div", "vada(5.0 / 2);", "2.5"),
    ("lit_mixed_add", "vada(1 + 2.5);", "3.5"),
    ("lit_mixed_mul", "vada(3 * 1.5);", "4.5"),
    ("lit_pow_frac", "vada(4 ** 0.5);", "2"),
    ("lit_eq_int", "vada(5 == 5);", "satya"),
    ("lit_ne_int", "vada(5 != 4);", "satya"),
    ("lit_eq_float_int", "vada(5 == 5.0);", "satya"),
    ("lit_lt", "vada(3 < 4);", "satya"),
    ("lit_le", "vada(4 <= 4);", "satya"),
    ("lit_gt", "vada(5 > 9);", "asatya"),
    ("lit_ge", "vada(5 >= 5);", "satya"),
    ("lit_eq_str", "vada(\"ka\" == \"ka\");", "satya"),
    ("lit_ne_str", "vada(\"ka\" != \"kha\");", "satya"),
    ("lit_chain_cmp", "vada(1 < 2 && 2 < 3);", "satya"),
    // ---- arith
    ("arith_add_1_2", "vada(1 + 2);", "3"),
    ("arith_add_3_4", "vada(3 + 4);", "7"),
    ("arith_add_7_8", "vada(7 + 8);", "15"),
    ("arith_add_10_20", "vada(10 + 20);", "30"),
    ("arith_add_99_1", "vada(99 + 1);", "100"),
    ("arith_add_123_456", "vada(123 + 456);", "579"),
    ("arith_add_0_7", "vada(0 + 7);", "7"),
    ("arith_add_8_0", "vada(8 + 0);", "8"),
    ("arith_add_1000_999", "vada(1000 + 999);", "1999"),
    ("arith_add_55_45", "vada(55 + 45);", "100"),
    ("arith_sub_5_2", "vada(5 - 2);", "3"),
    ("arith_sub_9_3", "vada(9 - 3);", "6"),
    ("arith_sub_10_7", "vada(10 - 7);", "3"),
    ("arith_sub_100_11", "vada(100 - 11);", "89"),
    ("arith_sub_42_6", "vada(42 - 6);", "36"),
    ("arith_sub_17_17", "vada(17 - 17);", "0"),
    ("arith_sub_80_9", "vada(80 - 9);", "71"),
    ("arith_sub_7_1", "vada(7 - 1);", "6"),
    ("arith_sub_500_250", "vada(500 - 250);", "250"),
    ("arith_sub_13_4", "vada(13 - 4);", "9"),
    ("arith_mul_2_3", "vada(2 * 3);", "6"),
    ("arith_mul_6_7", "vada(6 * 7);", "42"),
    ("arith_mul_11_13", "vada(11 * 13);", "143"),
    ("arith_mul_100_100", "vada(100 * 100);", "10000"),
    ("arith_mul_9_0", "vada(9 * 0);", "0"),
    ("arith_mul_12_12", "vada(12 * 12);", "144"),
    ("arith_mul_25_4", "vada(25 * 4);", "100"),
    ("arith_mul_7_8", "vada(7 * 8);", "56"),
    ("arith_mul_3_33", "vada(3 * 33);", "99"),
    ("arith_mul_50_2", "vada(50 * 2);", "100"),
    ("arith_mod_7_2", "vada(7 % 2);", "1"),
    ("arith_mod_20_3", "vada(20 % 3);", "2"),
    ("arith_mod_99_10", "vada(99 % 10);", "9"),
    ("arith_mod_100_7", "vada(100 % 7);", "2"),
    ("arith_mod_35_5", "vada(35 % 5);", "0"),
    ("arith_mod_1_1", "vada(1 % 1);", "0"),
    ("arith_mod_0_5", "vada(0 % 5);", "0"),
    ("arith_mod_123_11", "vada(123 % 11);", "2"),
    ("arith_mod_48_18", "vada(48 % 18);", "12"),
    ("arith_mod_17_4", "vada(17 % 4);", "1"),
    ("arith_pow_2_3", "vada(2 ** 3);", "8"),
    ("arith_pow_3_3", "vada(3 ** 3);", "27"),
    ("arith_pow_5_4", "vada(5 ** 4);", "625"),
    ("arith_pow_10_2", "vada(10 ** 2);", "100"),
    ("arith_pow_7_0", "vada(7 ** 0);", "1"),
    ("arith_pow_1_10", "vada(1 ** 10);", "1"),
    ("arith_pow_4_5", "vada(4 ** 5);", "1024"),
    ("arith_pow_6_2", "vada(6 ** 2);", "36"),
    ("arith_pow_9_3", "vada(9 ** 3);", "729"),
    ("arith_pow_11_1", "vada(11 ** 1);", "11"),
    // ---- str
    ("str_lit", "vada(\"namaste\");", "namaste"),
    ("str_concat2", "vada(\"nama\" + \"ste\");", "namaste"),
    ("str_concat3", "vada(\"ka\" + \"kha\" + \"ga\");", "kakhaga"),
    ("str_concat_num", "vada(\"mUlya=\" + 42);", "mUlya=42"),
    ("str_concat_bool", "vada(\"bUla=\" + satya);", "bUla=satya"),
    (
        "str_template_var",
        "astu nAma = \"rAma\"; vada(\"namaste {nAma}\");",
        "namaste rAma",
    ),
    ("str_template_expr", "vada(\"uttara={2 + 3}\");", "uttara=5"),
    (
        "str_template_multi",
        "astu a = 2; astu ba = 3; vada(\"y={a}+{ba}={a+ba}\");",
        "y=2+3=5",
    ),
    ("str_template_num", "vada(\"maiM {100} hUM\");", "maiM 100 hUM"),
    ("str_escape_quote", "vada(\"ka\\\"kha\");", "ka\"kha"),
    ("str_escape_backslash", "vada(\"ka\\\\kha\");", "ka\\kha"),
    ("str_escape_n", "vada(\"ka\\nkha\");", "ka"),
    ("str_escape_t", "vada(\"ka\\tkha\");", "ka"),
    ("str_raw", "vada(r\"ka\\nkha\");", "ka\\nkha"),
    ("str_triple", "vada(\"\"\"kakhaga\"\"\");", "kakhaga"),
    ("str_len_itrans", "vada(\"namaste\".dairghya());", "7"),
    ("str_len_ascii", "vada(\"abc\".len());", "3"),
    (
        "str_split_comma",
        "vada(\"ka,kha,ga\".vibhAjana(\",\"));",
        "[ka, kha, ga]",
    ),
    ("str_split_chars", "vada(\"aba\".vibhAjana(\"\"));", "[a, b, a]"),
    ("str_replace", "vada(\"kakhaka\".pratisthApana(\"ka\", \"ga\"));", "gakhaga"),
    ("str_lower", "vada(\"NAMASTE\".adhaH());", "namaste"),
    ("str_upper", "vada(\"namaste\".upari());", "NAMASTE"),
    ("str_trim", "vada(\"  namaste  \".chhaTanA());", "namaste"),
    ("str_slice", "vada(\"namaste\".khaNDa(1, 3));", "[a, m]"),
    ("str_empty", "vada(\"\");", ""),
    ("str_unicode", "vada(\"kShetra\");", "kShetra"),
    // ---- bool
    ("bool_true_print", "vada(satya);", "satya"),
    ("bool_false_print", "vada(asatya);", "asatya"),
    ("bool_null_print", "vada(shUnya);", "shUnya"),
    ("bool_not_true", "vada(na satya);", "asatya"),
    ("bool_not_false", "vada(na asatya);", "satya"),
    ("bool_not_zero", "vada(na 0);", "satya"),
    ("bool_not_str", "vada(na \"ka\");", "asatya"),
    ("bool_and_tt", "vada(satya cha satya);", "satya"),
    ("bool_and_tf", "vada(satya cha asatya);", "asatya"),
    ("bool_or_ff", "vada(asatya vA asatya);", "asatya"),
    ("bool_or_ft", "vada(asatya vA satya);", "satya"),
    ("bool_and_value", "vada(0 cha 5);", "0"),
    ("bool_or_value", "vada(5 vA 7);", "5"),
    ("bool_shortcircuit", "vada(asatya cha aparichita);", "asatya"),
    (
        "bool_if_truthy",
        "yadi (5) { vada(\"hAM\"); } anyathA { vada(\"nahIM\"); }",
        "hAM",
    ),
    (
        "bool_if_falsy",
        "yadi (0) { vada(\"hAM\"); } anyathA { vada(\"nahIM\"); }",
        "nahIM",
    ),
    ("bool_bang_not", "vada(!satya);", "asatya"),
    ("bool_andand", "vada(satya && satya);", "satya"),
    ("bool_oror", "vada(asatya || satya);", "satya"),
    // ---- null
    ("null_eq", "vada(shUnya == shUnya);", "satya"),
    (
        "null_truthy",
        "yadi (shUnya) { vada(\"x\"); } anyathA { vada(\"y\"); }",
        "y",
    ),
    // ---- bind
    ("bind_let", "astu na = 5; vada(na);", "5"),
    ("bind_let_alias", "bhavatu na = 6; vada(na);", "6"),
    ("bind_const", "nitya na = 7; vada(na);", "7"),
    ("bind_no_init", "astu na; vada(na);", "shUnya"),
    ("bind_reassign", "astu na = 1; na = 9; vada(na);", "9"),
    ("bind_compound_like", "astu na = 2; na = na * 5; vada(na);", "10"),
    (
        "bind_shadow",
        "astu na = 1; yadi (satya) { astu na = 2; vada(na); } vada(na);",
        "2",
    ),
    (
        "bind_multi",
        "astu a = 1; astu ba = 2; astu sa = a + ba; vada(sa);",
        "3",
    ),
    ("bind_str", "astu sa = \"namaste\"; vada(sa);", "namaste"),
    ("bind_list", "astu la = [1, 2]; vada(la);", "[1, 2]"),
    (
        "bind_func_value",
        "kriyA pha() { phalam 5; } astu ga = pha; vada(ga());",
        "5",
    ),
    (
        "bind_lambda",
        "astu f = kriyA(na) { phalam na + 1; }; vada(f(9));",
        "10",
    ),
    ("bind_underscore", "astu _na = 5; vada(_na);", "5"),
    ("bind_ascii", "astu foo = 5; vada(foo);", "5"),
    ("bind_assign_expr", "astu na = 0; vada(na = 7); vada(na);", "7"),
    // ---- if
    ("if_true", "yadi (satya) { vada(\"hAM\"); }", "hAM"),
    ("if_false", "yadi (asatya) { vada(\"hAM\"); }", ""),
    (
        "if_else_then",
        "yadi (satya) { vada(\"A\"); } anyathA { vada(\"B\"); }",
        "A",
    ),
    (
        "if_else_else",
        "yadi (asatya) { vada(\"A\"); } anyathA { vada(\"B\"); }",
        "B",
    ),
    (
        "if_elif_1",
        "astu na = 1; yadi (na == 1) { vada(\"eka\"); } anyathA yadi (na == 2) { vada(\"do\"); } anyathA { vada(\"anya\"); }",
        "eka",
    ),
    (
        "if_elif_2",
        "astu na = 2; yadi (na == 1) { vada(\"eka\"); } anyathA yadi (na == 2) { vada(\"do\"); } anyathA { vada(\"anya\"); }",
        "do",
    ),
    (
        "if_elif_3",
        "astu na = 3; yadi (na == 1) { vada(\"eka\"); } anyathA yadi (na == 2) { vada(\"do\"); } anyathA { vada(\"anya\"); }",
        "anya",
    ),
    (
        "if_nested_outer",
        "yadi (satya) { yadi (satya) { vada(\"inside\"); } }",
        "inside",
    ),
    (
        "if_nested_else",
        "yadi (satya) { yadi (asatya) { vada(\"a\"); } anyathA { vada(\"b\"); } }",
        "b",
    ),
    (
        "if_nested_deep",
        "yadi (1 < 2) { yadi (2 < 3) { yadi (3 < 4) { vada(\"deep\"); } } }",
        "deep",
    ),
    (
        "if_with_return",
        "kriyA pha(na) { yadi (na > 0) { phalam 1; } phalam 2; } vada(pha(5));",
        "1",
    ),
    (
        "if_with_and",
        "astu na = 5; yadi (na > 0 cha na < 10) { vada(\"mid\"); }",
        "mid",
    ),
    ("if_no_danda", "yadi (satya) { vada(1) }", "1"),
    // ---- while
    (
        "while_count",
        "astu na = 0; yAvat (na < 3) { vada(na); na = na + 1; }",
        "0",
    ),
    (
        "while_count_end",
        "astu na = 0; yAvat (na < 3) { na = na + 1; } vada(na);",
        "3",
    ),
    (
        "while_sum",
        "astu na = 1; astu sa = 0; yAvat (na <= 5) { sa = sa + na; na = na + 1; } vada(sa);",
        "15",
    ),
    (
        "while_break",
        "astu na = 0; yAvat (satya) { na = na + 1; yadi (na == 3) { virama; } } vada(na);",
        "3",
    ),
    (
        "while_break_first",
        "astu na = 0; yAvat (satya) { virama; } vada(na);",
        "0",
    ),
    (
        "while_continue",
        "astu na = 0; astu sa = 0; yAvat (na < 5) { na = na + 1; yadi (na == 2) { anuvarta; } sa = sa + na; } vada(sa);",
        "13",
    ),
    (
        "while_continue_even",
        "astu na = 0; astu sa = 0; yAvat (na < 6) { na = na + 1; yadi (na % 2 == 0) { anuvarta; } sa = sa + na; } vada(sa);",
        "9",
    ),
    (
        "while_nested",
        "astu AI = 0; astu sa = 0; yAvat (AI < 3) { astu je = 0; yAvat (je < 2) { sa = sa + 1; je = je + 1; } AI = AI + 1; } vada(sa);",
        "6",
    ),
    (
        "while_fib",
        "astu a = 0; astu ba = 1; astu ka = 0; yAvat (ka < 6) { astu ta = a + ba; a = ba; ba = ta; ka = ka + 1; } vada(a);",
        "8",
    ),
    (
        "while_gcd",
        "astu a = 48; astu ba = 18; yAvat (ba != 0) { astu ta = ba; ba = a % ba; a = ta; } vada(a);",
        "6",
    ),
    (
        "while_never",
        "astu na = 5; yAvat (na < 1) { vada(\"no\"); na = na + 1; } vada(\"done\");",
        "done",
    ),
    (
        "while_break_outer",
        "astu sa = 0; astu AI = 0; yAvat (AI < 5) { AI = AI + 1; yAvat (satya) { sa = sa + 1; virama; } } vada(sa);",
        "5",
    ),
    // ---- for
    ("for_range_exclusive", "pratyekam i antaH 1..4 { vada(i); }", "1"),
    (
        "for_range_last",
        "astu ka = 0; pratyekam i antaH 1..4 { ka = i; } vada(ka);",
        "3",
    ),
    (
        "for_range_inclusive",
        "astu ka = 0; pratyekam i antaH 1..=3 { ka = i; } vada(ka);",
        "3",
    ),
    (
        "for_range_sum",
        "astu sa = 0; pratyekam i antaH 1..=10 { sa = sa + i; } vada(sa);",
        "55",
    ),
    (
        "for_range_prod",
        "astu pa = 1; pratyekam i antaH 1..=5 { pa = pa * i; } vada(pa);",
        "120",
    ),
    ("for_list", "pratyekam i antaH [10, 20, 30] { vada(i); }", "10"),
    (
        "for_list_sum",
        "astu sa = 0; pratyekam i antaH [1, 2, 3, 4] { sa = sa + i; } vada(sa);",
        "10",
    ),
    ("for_str", "pratyekam cha antaH [\"ka\", \"kha\"] { vada(cha); }", "ka"),
    (
        "for_break",
        "astu ka = 0; pratyekam i antaH 1..=10 { ka = i; yadi (i == 4) { virama; } } vada(ka);",
        "4",
    ),
    (
        "for_continue",
        "astu sa = 0; pratyekam i antaH 1..=5 { yadi (i % 2 == 0) { anuvarta; } sa = sa + i; } vada(sa);",
        "9",
    ),
    (
        "for_nested",
        "astu sa = 0; pratyekam AI antaH 1..=2 { pratyekam je antaH 1..=3 { sa = sa + 1; } } vada(sa);",
        "6",
    ),
    (
        "for_empty_range",
        "pratyekam i antaH 5..5 { vada(\"no\"); } vada(\"done\");",
        "done",
    ),
    (
        "for_range_expr",
        "astu na = 3; astu sa = 0; pratyekam i antaH 1..(na+1) { sa = sa + i; } vada(sa);",
        "6",
    ),
    // ---- fn
    ("fn_simple", "kriyA pha() { phalam 5; } vada(pha());", "5"),
    (
        "fn_args",
        "kriyA yoga(a, ba) { phalam a + ba; } vada(yoga(3, 4));",
        "7",
    ),
    (
        "fn_three_args",
        "kriyA yoga(a, ba, sa) { phalam a + ba + sa; } vada(yoga(1, 2, 3));",
        "6",
    ),
    ("fn_default", "kriyA pha(a = 5) { phalam a; } vada(pha());", "5"),
    (
        "fn_default_override",
        "kriyA pha(a = 5) { phalam a; } vada(pha(9));",
        "9",
    ),
    (
        "fn_early_return",
        "kriyA pha(na) { yadi (na <= 0) { phalam 0; } phalam na * 2; } vada(pha(4));",
        "8",
    ),
    ("fn_return_no_expr", "kriyA pha() { phalam; } vada(pha());", "shUnya"),
    (
        "fn_implicit_null",
        "kriyA pha() { astu na = 1; } vada(pha());",
        "shUnya",
    ),
    (
        "fn_rec_fact",
        "kriyA phaika(na) { yadi (na <= 1) { phalam 1; } phalam na * phaika(na - 1); } vada(phaika(10));",
        "3628800",
    ),
    (
        "fn_rec_fib",
        "kriyA phiba(na) { yadi (na < 2) { phalam na; } phalam phiba(na-1) + phiba(na-2); } vada(phiba(15));",
        "610",
    ),
    (
        "fn_rec_gcd",
        "kriyA gresa(a, ba) { yadi (ba == 0) { phalam a; } phalam gresa(ba, a % ba); } vada(gresa(48, 18));",
        "6",
    ),
    (
        "fn_rec_sum",
        "kriyA sa(na) { yadi (na == 0) { phalam 0; } phalam na + sa(na-1); } vada(sa(10));",
        "55",
    ),
    (
        "fn_rec_power",
        "kriyA ghAta(a, na) { yadi (na == 0) { phalam 1; } phalam a * ghAta(a, na-1); } vada(ghAta(2, 10));",
        "1024",
    ),
    (
        "fn_mutual_rec",
        "kriyA sama(na) { yadi (na == 0) { phalam satya; } phalam viShama(na-1); } kriyA viShama(na) { yadi (na == 0) { phalam asatya; } phalam sama(na-1); } vada(sama(10));",
        "satya",
    ),
    (
        "fn_nested_call",
        "kriyA varga(a, ba) { phalam a + ba; } kriyA chaturguNa(na) { phalam varga(na, na) + varga(na, na); } vada(chaturguNa(5));",
        "20",
    ),
    (
        "fn_lambda",
        "astu f = kriyA(na) { phalam na * 2; }; vada(f(6));",
        "12",
    ),
    (
        "fn_lambda_multi",
        "astu f = kriyA(a, ba) { phalam a * ba; }; vada(f(3, 7));",
        "21",
    ),
    (
        "fn_hof_map",
        "kriyA anvaya(pha, sU) { astu ra = []; pratyekam i antaH sU { ra.prakShepa(pha(i)); } phalam ra; } vada(anvaya(kriyA(na){ phalam na*2; }, 1..4));",
        "[2, 4, 6]",
    ),
    (
        "fn_hof_filter",
        "kriyA chayana(pha, sU) { astu ra = []; pratyekam i antaH sU { yadi (pha(i)) { ra.prakShepa(i); } } phalam ra; } vada(chayana(kriyA(na){ phalam na % 2 == 0; }, 1..=6));",
        "[2, 4, 6]",
    ),
    (
        "fn_ret_str",
        "kriyA namaste(na) { phalam \"namaste, \" + na + \"!\"; } vada(namaste(\"vishva\"));",
        "namaste, vishva!",
    ),
    (
        "fn_typed",
        "kriyA pha(na : pUrNA~nka) -> pUrNA~nka { phalam na + 1; } vada(pha(4));",
        "5",
    ),
    // ---- closure
    (
        "closure_basic",
        "kriyA guNaka(ga) { phalam kriyA(na) { phalam na * ga; } } astu d = guNaka(2); vada(d(7));",
        "14",
    ),
    (
        "closure_two",
        "kriyA guNaka(ga) { phalam kriyA(na) { phalam na * ga; } } astu d = guNaka(2); astu t = guNaka(3); vada(d(7)); vada(t(7));",
        "14",
    ),
    (
        "closure_counter",
        "kriyA gaNaka() { astu na = 0; phalam kriyA() { na = na + 1; phalam na; } } astu g = gaNaka(); vada(g());",
        "1",
    ),
    (
        "closure_counter2",
        "kriyA gaNaka() { astu na = 0; phalam kriyA() { na = na + 1; phalam na; } } astu g = gaNaka(); vada(g()); vada(g()); vada(g());",
        "3",
    ),
    (
        "closure_capture_var",
        "astu ga = 10; astu f = kriyA(na) { phalam na + ga; }; vada(f(5));",
        "15",
    ),
    (
        "closure_capture_mutate",
        "astu ga = 10; astu f = kriyA() { ga = ga + 1; phalam ga; }; vada(f()); vada(f());",
        "12",
    ),
    (
        "closure_nested",
        "kriyA bAhya(ka) { phalam kriyA(kha) { phalam kriyA(ga) { phalam ka + kha + ga; } } } vada(bAhya(1)(2)(3));",
        "6",
    ),
    (
        "closure_in_loop",
        "astu fs = []; pratyekam i antaH 1..=3 { astu ka = i; fs.prakShepa(kriyA() { phalam ka * 10; }); } astu sa = 0; pratyekam f antaH fs { sa = sa + f(); } vada(sa);",
        "60",
    ),
    // ---- karaka
    (
        "karaka_layer_a",
        "kriyA preShaya(kartA ka, karma pa) { phalam ka + \":\" + pa; } vada(preShaya(karma: \"patra\", kartA: \"rAma\"));",
        "rAma:patra",
    ),
    (
        "karaka_layer_a_order",
        "kriyA preShaya(kartA ka, karma pa) { phalam ka + \":\" + pa; } vada(preShaya(kartA: \"rAma\", karma: \"patra\"));",
        "rAma:patra",
    ),
    (
        "karaka_layer_a_4",
        "kriyA preShaya(kartA ka, karma pa, karaNa ma, sampradAna ga) { phalam ka + \"|\" + pa + \"|\" + ma + \"|\" + ga; } vada(preShaya(sampradAna: \"shyAma\", karma: \"patra\", kartA: \"rAma\", karaNa: \"DAka\"));",
        "rAma|patra|DAka|shyAma",
    ),
    (
        "karaka_positional",
        "kriyA preShaya(kartA ka, karma pa) { phalam ka + \":\" + pa; } vada(preShaya(\"rAma\", \"patra\"));",
        "rAma:patra",
    ),
    (
        "karaka_named_param",
        "kriyA pha(ka, pa) { phalam ka + \":\" + pa; } vada(pha(pa: \"ba\", ka: \"a\"));",
        "a:ba",
    ),
    (
        "karaka_default",
        "kriyA pha(kartA ka = \"DiphaOlTa\") { phalam ka; } vada(pha());",
        "DiphaOlTa",
    ),
    (
        "karaka_default_override",
        "kriyA pha(kartA ka = \"DiphaOlTa\") { phalam ka; } vada(pha(kartA: \"diyA\"));",
        "diyA",
    ),
    (
        "karaka_layer_b_ins",
        "astu DAkena = \"DAka\"; kriyA bheja(karaNa ma) { phalam ma; } vada(bheja(DAkena));",
        "DAka",
    ),
    (
        "karaka_layer_b_dat",
        "astu shyAmAya = \"shyAma\"; kriyA bheja(sampradAna ga) { phalam ga; } vada(bheja(shyAmAya));",
        "shyAma",
    ),
    (
        "karaka_layer_b_nom",
        "astu rAmaH = \"rAma\"; kriyA bheja(kartA ka) { phalam ka; } vada(bheja(rAmaH));",
        "rAma",
    ),
    (
        "karaka_mixed",
        "kriyA pha(kartA ka, karma pa = \"patra\") { phalam ka + \":\" + pa; } vada(pha(\"rAma\"));",
        "rAma:patra",
    ),
    (
        "karaka_roles_all",
        "kriyA sUchanA(kartA ka, karma karmaNi, karaNa hetuH, sampradAna grAhakaH, apAdAna utpattiH, adhikaraNa sthAnam, sambandha sambandhI) { phalam ka; } vada(sUchanA(kartA: \"1\", karma: \"2\", karaNa: \"3\", sampradAna: \"4\", apAdAna: \"5\", adhikaraNa: \"6\", sambandha: \"7\"));",
        "1",
    ),
    // ---- sutra
    (
        "sutra_simple",
        "sUtra shulka(va) { phalam va * 10; } vada(shulka(4));",
        "40",
    ),
    (
        "sutra_apavada",
        "sUtra shulka(va) { phalam 10; } apavAda sUtra shulka(va) yadi (va > 50) { phalam 0; } vada(shulka(10)); vada(shulka(100));",
        "10",
    ),
    (
        "sutra_apavada_fires",
        "sUtra shulka(va) { phalam 10; } apavAda sUtra shulka(va) yadi (va > 50) { phalam 0; } vada(shulka(100));",
        "0",
    ),
    ("sutra_utsarga", "utsarga sUtra sa(va) { phalam 1; } vada(sa(2));", "1"),
    (
        "sutra_later_wins",
        "sUtra ga(va) { phalam 1; } sUtra ga(va) { phalam 2; } vada(ga(0));",
        "2",
    ),
    (
        "sutra_guard_false",
        "sUtra ga(va) { phalam 5; } apavAda sUtra ga(va) yadi (va < 0) { phalam 9; } vada(ga(1));",
        "5",
    ),
    (
        "sutra_guard_true",
        "sUtra ga(va) { phalam 5; } apavAda sUtra ga(va) yadi (va > 0) { phalam 9; } vada(ga(1));",
        "9",
    ),
    (
        "sutra_specificity",
        "sUtra ga(va) { phalam 1; } apavAda sUtra ga(va) yadi (va > 0) { phalam 2; } apavAda sUtra ga(va) yadi (va > 0 cha va < 10) { phalam 3; } vada(ga(5));",
        "3",
    ),
    (
        "sutra_multi_param",
        "sUtra yoga(a, ba) { phalam a + ba; } vada(yoga(3, 4));",
        "7",
    ),
    (
        "sutra_str",
        "sUtra varNa(sa) { phalam sa + \"!\"; } vada(varNa(\"namaste\"));",
        "namaste!",
    ),
    ("sutra_no_trace", "sUtra ga(va) { phalam va; } vada(ga(7));", "7"),
    // ---- list
    ("list_empty", "vada([]);", "[]"),
    ("list_lit", "vada([1, 2, 3]);", "[1, 2, 3]"),
    ("list_mixed", "vada([1, \"do\", satya]);", "[1, do, satya]"),
    ("list_index0", "vada([10, 20, 30][0]);", "10"),
    ("list_index2", "vada([10, 20, 30][2]);", "30"),
    (
        "list_index_expr",
        "astu la = [5, 6, 7]; astu AI = 1; vada(la[AI + 1]);",
        "7",
    ),
    (
        "list_set",
        "astu la = [10, 20]; la[0] = 99; vada(la);",
        "[99, 20]",
    ),
    (
        "list_push",
        "astu la = []; la.prakShepa(1); la.prakShepa(2); vada(la);",
        "[1, 2]",
    ),
    (
        "list_push_multi",
        "astu la = [1]; la.prakShepa(2, 3); vada(la);",
        "[1, 2, 3]",
    ),
    ("list_len", "vada([1, 2, 3, 4].dairghya());", "4"),
    ("list_len_alias", "vada([1, 2].len());", "2"),
    ("list_get", "vada([5, 6].prApta(1));", "6"),
    (
        "list_reverse",
        "astu la = [1, 2, 3]; la.viloma(); vada(la);",
        "[3, 2, 1]",
    ),
    (
        "list_sort",
        "astu la = [3, 1, 2]; la.vinyAsa(); vada(la);",
        "[1, 2, 3]",
    ),
    ("list_slice", "vada([1, 2, 3, 4, 5].khaNDa(1, 3));", "[2, 3]"),
    (
        "list_slice_open",
        "vada([1, 2, 3, 4, 5].khaNDa(2, 9));",
        "[3, 4, 5]",
    ),
    (
        "list_join",
        "vada([\"ka\", \"kha\", \"ga\"].yojanA(\"-\"));",
        "ka-kha-ga",
    ),
    ("list_join_empty", "vada([\"ka\", \"kha\"].yojanA(\"\"));", "kakha"),
    ("list_nested", "vada([[1, 2], [3, 4]]);", "[[1, 2], [3, 4]]"),
    ("list_nested_index", "vada([[1, 2], [3, 4]][1][0]);", "3"),
    (
        "list_truthy",
        "yadi ([1]) { vada(\"yes\"); } anyathA { vada(\"no\"); }",
        "yes",
    ),
    (
        "list_empty_falsy",
        "yadi ([]) { vada(\"yes\"); } anyathA { vada(\"no\"); }",
        "no",
    ),
    ("list_eq", "vada([1, 2] == [1, 2]);", "satya"),
    // ---- map
    ("map_empty", "vada({});", "{}"),
    ("map_lit", "astu ma = {\"ka\": 1, \"kha\": 2}; vada(ma);", "ka: 1"),
    ("map_index", "astu ma = {\"ka\": 1}; vada(ma[\"ka\"]);", "1"),
    ("map_index_missing", "vada({\"ka\": 1}.prApta(\"kha\"));", "shUnya"),
    (
        "map_set_method",
        "astu ma = {}; ma.prakShepa(\"ka\", 5); vada(ma[\"ka\"]);",
        "5",
    ),
    (
        "map_set_update",
        "astu ma = {\"ka\": 1}; ma.prakShepa(\"ka\", 9); vada(ma[\"ka\"]);",
        "9",
    ),
    ("map_get_method", "vada({\"ka\": 7}.prApta(\"ka\"));", "7"),
    ("map_keys", "vada({\"ka\": 1, \"kha\": 2}.kIsUchI());", "[ka, kha]"),
    ("map_len", "vada({\"ka\": 1, \"kha\": 2}.dairghya());", "2"),
    (
        "map_nested",
        "astu ma = {\"ka\": {\"kha\": 5}}; vada(ma[\"ka\"][\"kha\"]);",
        "5",
    ),
    ("map_int_key", "astu ma = {1: \"eka\"}; vada(ma[1]);", "eka"),
    ("map_bool_val", "vada({\"ka\": satya});", "ka: satya"),
    (
        "map_truthy",
        "yadi ({\"ka\": 1}) { vada(\"yes\"); } anyathA { vada(\"no\"); }",
        "yes",
    ),
    // ---- tuple
    ("tuple_empty", "vada(());", "()"),
    ("tuple_lit", "vada((1, \"do\", satya));", "(1, do, satya)"),
    ("tuple_index", "vada((10, 20)[1]);", "20"),
    ("tuple_nested", "vada(((1, 2), (3, 4)));", "((1, 2), (3, 4))"),
    ("tuple_eq", "vada((1, 2) == (1, 2));", "satya"),
    (
        "tuple_from_fn",
        "kriyA joDaI(a, ba) { phalam (a, ba); } vada(joDaI(3, 4));",
        "(3, 4)",
    ),
    ("tuple_in_list", "vada([(1, 2), (3, 4)]);", "[(1, 2), (3, 4)]"),
    (
        "tuple_truthy",
        "yadi ((1, 2)) { vada(\"yes\"); } anyathA { vada(\"no\"); }",
        "yes",
    ),
    // ---- class
    (
        "class_field_default",
        "varga biMdu { x = 1; y = 2; } vada(biMdu().x);",
        "1",
    ),
    (
        "class_field_two",
        "varga biMdu { x = 1; y = 2; } astu ba = biMdu(); vada(ba.x); vada(ba.y);",
        "1",
    ),
    (
        "class_ctor",
        "varga biMdu { x = 0; nirmANa(va) { svayam.x = va; } } vada(biMdu(7).x);",
        "7",
    ),
    (
        "class_method",
        "varga vRRita { r = 2; kShetra() { phalam svayam.r * svayam.r; } } vada(vRRita().kShetra());",
        "4",
    ),
    (
        "class_method_args",
        "varga gaNaka { AdhAra = 10; yoga(na) { phalam svayam.AdhAra + na; } } vada(gaNaka().yoga(5));",
        "15",
    ),
    (
        "class_field_assign",
        "varga biMdu { x = 1; } astu ba = biMdu(); ba.x = 9; vada(ba.x);",
        "9",
    ),
    (
        "class_inherit_field",
        "varga jaMtu { } varga kuttA : jaMtu { nAma = \"kuttA\"; } vada(kuttA().nAma);",
        "kuttA",
    ),
    (
        "class_inherit_method",
        "varga jaMtu { bola() { phalam \"AvAja\"; } } varga kuttA : jaMtu { } vada(kuttA().bola());",
        "AvAja",
    ),
    (
        "class_override",
        "varga jaMtu { bola() { phalam \"AvAja\"; } } varga kuttA : jaMtu { bola() { phalam \"bhauM\"; } } vada(kuttA().bola());",
        "bhauM",
    ),
    (
        "class_super_ctor",
        "varga jaMtu { nirmANa(na) { svayam.nAma = na; } } varga kuttA : jaMtu { nirmANa(na) { mUla.nirmANa(na); } pa() { phalam svayam.nAma; } } vada(kuttA(\"rAma\").pa());",
        "rAma",
    ),
    (
        "class_super_method",
        "varga jaMtu { bola() { phalam \"AvAja\"; } } varga kuttA : jaMtu { bola() { phalam \"bhauM-\" + mUla.bola(); } } vada(kuttA().bola());",
        "bhauM-AvAja",
    ),
    (
        "class_two_instances",
        "varga biMdu { x = 0; nirmANa(va) { svayam.x = va; } } astu pa = biMdu(1); astu ka = biMdu(2); vada(pa.x + ka.x);",
        "3",
    ),
    (
        "class_method_chain",
        "varga gaNaka { na = 0; vRRiddhi() { svayam.na = svayam.na + 1; phalam svayam; } mAna() { phalam svayam.na; } } vada(gaNaka().vRRiddhi().vRRiddhi().mAna());",
        "2",
    ),
    (
        "class_3_level",
        "varga a { pa() { phalam \"a\"; } } varga ba : a { } varga sa : ba { } vada(sa().pa());",
        "a",
    ),
    // ---- try
    (
        "try_catch",
        "yatna { utthApaya(doSha(\"oha\")); } doShe (I) { vada(I.sandesha); }",
        "oha",
    ),
    (
        "try_catch_kind",
        "yatna { utthApaya(doSha(\"x\")); } doShe (I) { vada(I.kind); }",
        "doSha",
    ),
    (
        "try_finally",
        "yatna { vada(\"shurU\"); } ante { vada(\"anta\"); }",
        "anta",
    ),
    (
        "try_catch_finally",
        "yatna { utthApaya(doSha(\"boom\")); } doShe (I) { vada(I.sandesha); } ante { vada(\"cleanup\"); }",
        "cleanup",
    ),
    (
        "try_no_error",
        "yatna { vada(\"ThIka\"); } doShe (I) { vada(\"galata\"); }",
        "ThIka",
    ),
    (
        "try_runtime_fault",
        "yatna { vada(1 / 0); } doShe (I) { vada(I.kind); }",
        "E0012",
    ),
    (
        "try_index_fault",
        "yatna { vada([1][7]); } doShe (I) { vada(I.kind); }",
        "E0013",
    ),
    (
        "try_nested",
        "yatna { yatna { utthApaya(doSha(\"inner\")); } doShe (I) { vada(I.sandesha); } vada(\"outer-ok\"); } doShe (I) { vada(\"bad\"); }",
        "outer-ok",
    ),
    (
        "try_in_fn",
        "kriyA pha() { yatna { utthApaya(doSha(\"e\")); } doShe (I) { phalam I.sandesha; } phalam \"none\"; } vada(pha());",
        "e",
    ),
    // ---- raise
    (
        "raise_custom_kind",
        "yatna { utthApaya(doSha(\"x\")); } doShe (I) { yadi (I.kind == \"doSha\") { vada(\"matched\"); } }",
        "matched",
    ),
    // ---- module
    ("module_math_sqrt", "vada(gaNita.vargamUla(16));", "4"),
    ("module_math_abs", "vada(gaNita.nirapekSha(-5));", "5"),
    ("module_math_min", "vada(gaNita.nyUnatama(5, 2, 8));", "2"),
    ("module_math_max", "vada(gaNita.adhikatama(5, 2, 8));", "8"),
    ("module_math_pow", "vada(gaNita.shakti(2, 3));", "8"),
    ("module_math_round", "vada(gaNita.gola(2.6));", "3"),
    (
        "module_import_alias",
        "Anaya \"gaNita\" -> ga; vada(ga.adhikatama(2, 9));",
        "9",
    ),
    (
        "module_import_plain",
        "Anaya \"gaNita\"; vada(gaNita.nyUnatama(3, 1));",
        "1",
    ),
    ("module_export", "nitya na = 5; prakAshaya na; vada(na);", "5"),
    (
        "module_sanskrit",
        "Anaya \"saMskRRita\" -> sa; vada(sa.pratyAhAra(\"ach\"));",
        "aiuRRiLLieoaiau",
    ),
    (
        "module_util",
        "Anaya \"su\" -> u; astu cha = u.chainala(); cha.preShaya(\"ok\"); vada(cha.prApta());",
        "ok",
    ),
    // ---- stdlib
    ("stdlib_print_multi", "vada(1, 2, 3);", "1 2 3"),
    ("stdlib_type_int", "vada(asti(5));", "pUrNA~nka"),
    ("stdlib_type_str", "vada(asti(\"ka\"));", "shabda"),
    ("stdlib_type_list", "vada(asti([]));", "sUchI"),
    ("stdlib_tostring", "vada(rUpAntara(123));", "123"),
    ("stdlib_abs_float", "vada(gaNita.nirapekSha(-3.5));", "3.5"),
    ("stdlib_sqrt2", "vada(gaNita.vargamUla(2) > 1);", "satya"),
    (
        "stdlib_random_seeded",
        "su.bIja(42); astu a = su.yAdRRichchhika(10); su.bIja(42); astu ba = su.yAdRRichchhika(10); vada(a == ba);",
        "satya",
    ),
    (
        "stdlib_random_range",
        "su.bIja(1); astu na = su.yAdRRichchhika(5); vada(na >= 0 && na < 5);",
        "satya",
    ),
    ("stdlib_json_stringify", "vada(su.nirmANa([1, 2]));", "[1,2]"),
    ("stdlib_json_obj", "vada(su.nirmANa({\"ka\": 1}));", "{\"ka\":1}"),
    (
        "stdlib_json_parse",
        "astu la = su.vishleShaNa(\"[1, 2]\"); vada(la[1]);",
        "2",
    ),
    ("stdlib_regex_true", "vada(su.milAna(\"nama\", \"namaste\"));", "satya"),
    (
        "stdlib_regex_false",
        "vada(su.milAna(\"^xyz\", \"namaste\"));",
        "asatya",
    ),
    ("stdlib_fs_exists", "vada(su.astitva(\"/tmp\"));", "satya"),
    ("stdlib_yield", "su.virAma(); vada(\"ok\");", "ok"),
    ("stdlib_join_tasks", "su.joDa(); vada(\"ok\");", "ok"),
    (
        "stdlib_channel_direct",
        "astu cha = su.chainala(); cha.preShaya(\"hI\"); vada(cha.prApta());",
        "hI",
    ),
    // ---- sanskrit
    (
        "sanskrit_translit_iast",
        "vada(saMskRRita.rUpAntaraNa(\"namaste\", \"itrans\", \"iast\"));",
        "namaste",
    ),
    (
        "sanskrit_translit_itrans",
        "vada(saMskRRita.rUpAntaraNa(\"vidyā\", \"iast\", \"itrans\"));",
        "vidyA",
    ),
    (
        "sanskrit_translit_hk",
        "vada(saMskRRita.rUpAntaraNa(\"namaste\", \"itrans\", \"hk\"));",
        "namaste",
    ),
    (
        "sanskrit_pratyahara_ac",
        "vada(saMskRRita.pratyAhAra(\"ach\"));",
        "aiuRRiLLieoaiau",
    ),
    (
        "sanskrit_pratyahara_hal",
        "vada(saMskRRita.pratyAhAra(\"al\"));",
        "hayavarala",
    ),
    (
        "sanskrit_akshara_namaste",
        "vada(saMskRRita.akSharANi(\"namaste\"));",
        "[na, ma, ste]",
    ),
    (
        "sanskrit_akshara_ramah",
        "vada(saMskRRita.akSharANi(\"rAmaH\"));",
        "[rA, maH]",
    ),
    ("sanskrit_matra_namaste", "vada(saMskRRita.mAtrA(\"namaste\"));", "LLG"),
    ("sanskrit_matra_ramah", "vada(saMskRRita.mAtrA(\"rAmaH\"));", "GG"),
    (
        "sanskrit_vibhakti_nom",
        "vada(saMskRRita.vibhakti(\"rAma\", \"a\", \"m\", \"sg\", \"nom\"));",
        "rAmaH",
    ),
    (
        "sanskrit_vibhakti_acc",
        "vada(saMskRRita.vibhakti(\"rAma\", \"a\", \"m\", \"sg\", \"acc\"));",
        "rAmam",
    ),
    (
        "sanskrit_vibhakti_ins",
        "vada(saMskRRita.vibhakti(\"rAma\", \"a\", \"m\", \"sg\", \"ins\"));",
        "rAmeNa",
    ),
    (
        "sanskrit_vibhakti_dat",
        "vada(saMskRRita.vibhakti(\"rAma\", \"a\", \"m\", \"sg\", \"dat\"));",
        "rAmAya",
    ),
    (
        "sanskrit_vibhakti_abl",
        "vada(saMskRRita.vibhakti(\"rAma\", \"a\", \"m\", \"sg\", \"abl\"));",
        "rAmAt",
    ),
    (
        "sanskrit_vibhakti_gen",
        "vada(saMskRRita.vibhakti(\"rAma\", \"a\", \"m\", \"sg\", \"gen\"));",
        "rAmasya",
    ),
    (
        "sanskrit_vibhakti_loc",
        "vada(saMskRRita.vibhakti(\"rAma\", \"a\", \"m\", \"sg\", \"loc\"));",
        "rAme",
    ),
    (
        "sanskrit_vibhakti_parse",
        "vada(saMskRRita.vibhaktiparichchheda(\"rAmaH\"));",
        "[rAma/0]",
    ),
    (
        "sanskrit_sandhi_join",
        "vada(saMskRRita.sandhi(\"a\", \"asti\")[0]);",
        "Asti",
    ),
    (
        "sanskrit_sandhi_split",
        "vada(saMskRRita.sandhivichchheda(\"Asti\")[0]);",
        "(a, sti)",
    ),
    (
        "sanskrit_metre",
        "vada(saMskRRita.chhandaH(\"nanananananAnana;nanananananAnana;nanananananAnana;nanananananAnana;\"));",
        "anuShTubh",
    ),
    ("sanskrit_metre_none", "vada(saMskRRita.chhandaH(\"namaste\"));", "shUnya"),
    // ---- task
    (
        "task_two_producers",
        "astu cha = su.chainala(); tantu utpAdaka { cha.preShaya(\"namaste\"); cha.preShaya(\"vishva\"); } tantu upabhoktA { vada(cha.prApta()); vada(cha.prApta()); }",
        "namaste",
    ),
    (
        "task_yield",
        "astu cha = su.chainala(); tantu u { su.virAma(); cha.preShaya(\"ok\"); } tantu va { vada(cha.prApta()); }",
        "ok",
    ),
    (
        "task_three",
        "astu cha = su.chainala(); tantu ka { cha.preShaya(1); } tantu kha { cha.preShaya(2); } tantu ga { vada(cha.prApta() + cha.prApta()); }",
        "3",
    ),
    ("task_print", "tantu ka { vada(\"from-task\"); }", "from-task"),
    (
        "task_two_print",
        "tantu ka { vada(\"ka\"); } tantu kha { vada(\"kha\"); }",
        "ka",
    ),
    // ---- channel
    (
        "channel_fifo",
        "astu cha = su.chainala(); cha.preShaya(1); cha.preShaya(2); vada(cha.prApta()); vada(cha.prApta());",
        "1",
    ),
    // ---- typing
    ("typing_let_int", "astu na : pUrNA~nka = 5; vada(na);", "5"),
    ("typing_let_str", "astu sa : shabda = \"namaste\"; vada(sa);", "namaste"),
    ("typing_let_bool", "astu ba : bUliyana = satya; vada(ba);", "satya"),
    (
        "typing_param",
        "kriyA pha(na : pUrNA~nka) { phalam na + 1; } vada(pha(2));",
        "3",
    ),
    (
        "typing_return",
        "kriyA pha(na : pUrNA~nka) -> pUrNA~nka { phalam na * 2; } vada(pha(6));",
        "12",
    ),
    ("typing_list", "astu la : sUchI = [1, 2]; vada(la);", "[1, 2]"),
    ("typing_map", "astu ma : kosha = {\"ka\": 1}; vada(ma);", "ka: 1"),
    // ---- assert
    ("assert_eq_ok", "parIkShA.sama(2, 2); vada(\"passed\");", "passed"),
    (
        "assert_eq_str",
        "parIkShA.sama(\"ka\", \"ka\"); vada(\"passed\");",
        "passed",
    ),
    (
        "assert_fail_caught",
        "yatna { parIkShA.viphala(\"boom\"); } doShe (I) { vada(I.kind); }",
        "E0017",
    ),
    (
        "assert_cmp_ok",
        "parIkShA.sama([1, 2], [1, 2]); vada(\"passed\");",
        "passed",
    ),
    (
        "assert_in_fn",
        "kriyA jAcha(na) { parIkShA.sama(na, na); phalam \"ok\"; } vada(jAcha(5));",
        "ok",
    ),
    // ---- comment
    ("comment_line", "// TippaNI\nvada(1);", "1"),
    ("comment_block", "/* bahu\n TippaNI */ vada(2);", "2"),
    ("comment_nested", "/* bAhya /* Antara */ bAhya */ vada(3);", "3"),
    // ---- alias
    ("alias_astu", "bhavatu na = 5; vada(na);", "5"),
    ("alias_vidhi", "vidhi pha(na) { phalam na + 1; } vada(pha(2));", "3"),
    ("alias_semicolon", "vada(1); vada(2);", "1"),
    // ---- danda
    ("danda_double", "vada(1);", "1"),
    // ---- expr
    ("expr_statement", "1 + 2; vada(3);", "3"),
    // ---- pipeline
    (
        "pipeline_sieve",
        "astu sa = 0; pratyekam i antaH 1..=5 { sa = sa + i * i; } vada(sa);",
        "55",
    ),
    (
        "pipeline_bubble",
        "astu la = [5, 2, 9, 1]; astu AI = 0; yAvat (AI < la.dairghya()) { astu je = AI + 1; yAvat (je < la.dairghya()) { yadi (la[AI] > la[je]) { astu ta = la[AI]; la[AI] = la[je]; la[je] = ta; } je = je + 1; } AI = AI + 1; } vada(la);",
        "[1, 2, 5, 9]",
    ),
    (
        "pipeline_wordcount",
        "astu sa = \"namaste vishva\"; vada(sa.vibhAjana(\" \").dairghya());",
        "2",
    ),
    ("pipeline_caesar", "vada(\"abc\".upari());", "ABC"),
    (
        "pipeline_fizzbuzzish",
        "astu sa = 0; pratyekam i antaH 1..=15 { yadi (i % 3 == 0) { sa = sa + 1; } } vada(sa);",
        "5",
    ),
];

fn check((name, src, expected): &Case) {
    let out = pipeline::run_source(src, name, &Options::default())
        .unwrap_or_else(|e| panic!("[{name}] run_source returned Err: {e}"));
    assert!(
        out.diagnostics.is_empty(),
        "[{name}] unexpected diagnostics:\n{}\nsource:\n{src}",
        out.diagnostics
    );
    assert!(
        out.output.contains(expected),
        "[{name}] expected output containing {expected:?}, got:\n{}\nsource:\n{src}",
        out.output
    );
}

#[test]
fn conformance_suite() {
    assert!(CASES.len() >= 250, "only {} cases", CASES.len());
    let mut names = std::collections::HashSet::new();
    for case in CASES {
        assert!(names.insert(case.0), "duplicate case name {}", case.0);
        check(case);
    }
    eprintln!("ran {} conformance cases", CASES.len());
}
