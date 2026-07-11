let total xs =
  let acc = ref 0 in
  let add a = acc := !acc + a in
  List.iter add xs;
  !acc
