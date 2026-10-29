use serde_json::{json, Value};

use super::emit_ld_value;

fn edge(
    source: &str,
    target: &str,
    source_handle: Option<&str>,
    target_handle: Option<&str>,
) -> Value {
    json!({
        "id": format!("e-{source}-{target}-{}-{}", source_handle.unwrap_or(""), target_handle.unwrap_or("")),
        "source": source,
        "target": target,
        "sourceHandle": source_handle,
        "targetHandle": target_handle,
    })
}

fn rail(id: &str, variant: &str, x: i64) -> Value {
    json!({"id":id,"type":"powerRail","position":{"x":x,"y":30},"data":{"variant":variant}})
}

fn input(id: &str, name: &str, x: i64, y: i64) -> Value {
    json!({"id":id,"type":"variable","position":{"x":x,"y":y},"data":{"variant":"input","variable":{"name":name},"executionOrder":0,"numericId":id}})
}

fn output(id: &str, name: &str, x: i64, y: i64) -> Value {
    json!({"id":id,"type":"variable","position":{"x":x,"y":y},"data":{"variant":"output","variable":{"name":name},"executionOrder":0,"numericId":id}})
}

fn function(
    id: &str,
    name: &str,
    numeric: &str,
    order: i64,
    x: i64,
    y: i64,
    out_type: &str,
) -> Value {
    json!({
        "id":id,"type":"block","position":{"x":x,"y":y},
        "data":{
            "numericId":numeric,"executionOrder":order,"executionControl":true,
            "variant":{"name":name,"type":"function","extensible":false,"variables":[
                {"name":"EN","class":"input","type":{"definition":"generic-type","value":"BOOL"}},
                {"name":"ENO","class":"output","type":{"definition":"generic-type","value":"BOOL"}},
                {"name":"OUT","class":"output","type":{"definition":"base-type","value":out_type}},
                {"name":"IN1","class":"input","type":{"definition":"base-type","value":"DINT"}},
                {"name":"IN2","class":"input","type":{"definition":"base-type","value":"DINT"}}
            ]}
        }
    })
}

fn coil(id: &str, name: &str, variant: &str, x: i64, y: i64, numeric: &str) -> Value {
    json!({"id":id,"type":"coil","position":{"x":x,"y":y},"data":{"variant":variant,"variable":{"name":name},"executionOrder":0,"numericId":numeric}})
}

fn contact(id: &str, name: &str, x: i64, numeric: &str) -> Value {
    json!({"id":id,"type":"contact","position":{"x":x,"y":38},"data":{"variant":"default","variable":{"name":name},"numericId":numeric}})
}

fn parallel(id: &str, side: &str, x: i64) -> Value {
    json!({"id":id,"type":"parallel","position":{"x":x,"y":49},"data":{"type":side}})
}

#[test]
fn issue_830_eager_upstream_calls_interleave_output_writes() {
    let body = json!({"rungs":[{
        "reactFlowViewport":[800,300],
        "nodes":[
            rail("L","left",0),
            input("901","valor",20,80),
            input("902","k",20,120),
            function("MUL","MUL","111",0,150,30,"DINT"),
            output("903","producto",320,200),
            input("904","producto",20,160),
            input("905","k2",20,200),
            function("SUB","SUB","222",0,150,130,"DINT"),
            output("906","dif",320,260),
            function("GT","GT","333",0,500,30,"BOOL"),
            output("907","flag",660,30),
            rail("R","right",760)
        ],
        "edges":[
            edge("L","MUL",Some("left-rail"),Some("EN")),
            edge("901","MUL",None,Some("IN1")),
            edge("902","MUL",None,Some("IN2")),
            edge("MUL","903",Some("OUT"),None),
            edge("904","SUB",None,Some("IN1")),
            edge("905","SUB",None,Some("IN2")),
            edge("SUB","906",Some("OUT"),None),
            edge("MUL","GT",Some("ENO"),Some("EN")),
            edge("SUB","GT",Some("ENO"),Some("EN")),
            edge("GT","907",Some("OUT"),None)
        ]
    }]});
    let result = emit_ld_value(&body).unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    assert_eq!(
        result.body_st,
        "\n  _TMP_MUL111_OUT := MUL(EN := TRUE, IN1 := valor, IN2 := k, ENO => _TMP_MUL111_ENO);\n  IF _TMP_MUL111_ENO THEN\n      producto := _TMP_MUL111_OUT;\n  END_IF;\n  _TMP_SUB222_OUT := SUB(IN1 := producto, IN2 := k2, ENO => _TMP_SUB222_ENO);\n  dif := _TMP_SUB222_OUT;\n  _TMP_GT333_OUT := GT(EN := _TMP_MUL111_ENO OR _TMP_SUB222_ENO, ENO => _TMP_GT333_ENO);\n  IF _TMP_GT333_ENO THEN\n      flag := _TMP_GT333_OUT;\n  END_IF;\n"
    );
}

#[test]
fn issue_830_block_fed_coil_writes_before_downstream_call() {
    let body = json!({"rungs":[{
        "reactFlowViewport":[800,300],
        "nodes":[
            rail("L","left",0), input("901","Input",20,80), input("902","86400",20,120),
            function("GEA","GE","111",1,150,30,"BOOL"), coil("cFlag","Flag","default",320,30,"cFlag"),
            input("904","Flag",20,200), input("905","60",20,240),
            function("GEB","GE","222",2,450,30,"BOOL"), coil("cRes","Result","default",620,30,"cRes"), rail("R","right",760)
        ],
        "edges":[
            edge("L","GEA",Some("left-rail"),Some("EN")), edge("901","GEA",None,Some("IN1")), edge("902","GEA",None,Some("IN2")),
            edge("GEA","cFlag",Some("OUT"),None), edge("GEA","GEB",Some("ENO"),Some("EN")),
            edge("904","GEB",None,Some("IN1")), edge("905","GEB",None,Some("IN2")), edge("GEB","cRes",Some("OUT"),None)
        ]
    }]});
    let result = emit_ld_value(&body).unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    assert_eq!(
        result.body_st,
        "\n  _TMP_GE111_OUT := GE(EN := TRUE, IN1 := Input, IN2 := 86400, ENO => _TMP_GE111_ENO);\n  Flag := _TMP_GE111_OUT;\n  _TMP_GE222_OUT := GE(EN := _TMP_GE111_ENO, IN1 := Flag, IN2 := 60, ENO => _TMP_GE222_ENO);\n  Result := _TMP_GE222_OUT;\n"
    );
}

#[test]
fn issue_836_groups_three_coils_from_one_source() {
    let body = json!({"rungs":[{
        "reactFlowViewport":[600,300],
        "nodes":[rail("L","left",0),contact("C","FirstScan",68,"10"),parallel("PO","open",251),
            coil("K1","Output1","set",300,38,"20"),coil("KR","FirstScan","reset",300,130,"22"),coil("K2","Output2","set",300,220,"21"),
            parallel("PC","close",400),rail("R","right",500)],
        "edges":[edge("L","C",None,None),edge("C","PO",None,None),edge("PO","K1",None,None),edge("PO","KR",None,None),edge("PO","K2",None,None),
            edge("K1","PC",None,None),edge("KR","PC",None,None),edge("K2","PC",None,None),edge("PC","R",None,None)]
    }]});
    let result = emit_ld_value(&body).unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    assert_eq!(result.body_st, "\n  IF FirstScan THEN\n    Output1 := TRUE; (*set*)\n    FirstScan := FALSE; (*reset*)\n    Output2 := TRUE; (*set*)\n  END_IF;\n");
}

#[test]
fn issue_836_keeps_sequential_coils_separate() {
    let body = json!({"rungs":[{
        "reactFlowViewport":[700,100],
        "nodes":[rail("L","left",0),contact("C","FirstScan",68,"10"),coil("K1","Output1","set",200,38,"20"),coil("KR","FirstScan","reset",350,38,"22"),coil("K2","Output2","set",500,38,"21"),rail("R","right",650)],
        "edges":[edge("L","C",None,None),edge("C","K1",None,None),edge("K1","KR",None,None),edge("KR","K2",None,None),edge("K2","R",None,None)]
    }]});
    let result = emit_ld_value(&body).unwrap();
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    assert_eq!(result.body_st, "\n  IF FirstScan THEN\n    Output1 := TRUE; (*set*)\n  END_IF;\n  IF FirstScan THEN\n    FirstScan := FALSE; (*reset*)\n  END_IF;\n  IF FirstScan THEN\n    Output2 := TRUE; (*set*)\n  END_IF;\n");
}
