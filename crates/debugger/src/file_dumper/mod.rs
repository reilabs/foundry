//! The file dumper implementation

use alloy_primitives::{Address, Bytes, U256};
use serde::Serialize;
use std::{collections::HashMap, path::PathBuf};

use crate::context::DebuggerContext;
use eyre::Result;
use foundry_common::{compile::ContractSources, fs::write_json_file};
use foundry_compilers::artifacts::ContractBytecodeSome;
use foundry_evm_core::{
    debug::{DebugNodeFlat, DebugStep, Instruction},
    utils::PcIcMap,
};
use revm_inspectors::tracing::types::CallKind;

/// The file dumper
pub struct FileDumper<'a> {
    path: &'a PathBuf,
    debugger_context: &'a mut DebuggerContext,
}

impl<'a> FileDumper<'a> {
    pub fn new(path: &'a PathBuf, debugger_context: &'a mut DebuggerContext) -> Self {
        Self { path, debugger_context }
    }

    pub fn run(&mut self) -> Result<()> {
        let data = DebuggerDump::from(self.debugger_context)?;
        write_json_file(self.path, &data).unwrap();
        // let filename = format!(
        //     "./{}.json",
        //     self.debugger_context.contracts_sources.ids_by_name.keys().count()
        // );
        // let out_path = Path::new(&filename);
        // write_json_file(&out_path, &self.debugger_context.contracts_sources).unwrap();
        Ok(())
    }
}

impl DebuggerDump {
    fn from(debugger_context: &DebuggerContext) -> Result<DebuggerDump> {
        let contracts = to_contracts_dump(debugger_context)?;
        let executions = to_executions_dump(debugger_context);
        Ok(Self { contracts, executions })
    }
}

#[derive(Serialize)]
struct DebuggerDump {
    contracts: ContractsDump,
    executions: ExecutionsDump,
}

#[derive(Serialize)]
struct ExecutionsDump {
    calls: Vec<CallDump>,
    // Map of contract name to PcIcMapDump
    pc_ic_maps: HashMap<String, PcIcMapDump>,
}

#[derive(Serialize)]
struct CallDump {
    address: Address,
    kind: CallKind,
    steps: Vec<StepDump>,
}

#[derive(Serialize)]
struct StepDump {
    /// Stack *prior* to running the associated opcode
    stack: Vec<U256>,
    /// Memory *prior* to running the associated opcode
    memory: Bytes,
    /// Calldata *prior* to running the associated opcode
    calldata: Bytes,
    /// Returndata *prior* to running the associated opcode
    returndata: Bytes,
    /// Opcode to be executed
    instruction: Instruction,
    /// Optional bytes that are being pushed onto the stack
    push_bytes: Bytes,
    /// The program counter at this step.
    pc: usize,
    /// Cumulative gas usage
    total_gas_used: u64,
}

#[derive(Serialize)]
struct PcIcMapDump {
    create_code_map: HashMap<usize, usize>,
    runtime_code_map: HashMap<usize, usize>,
}

#[derive(Serialize)]
struct ContractsDump {
    // Map of call address to contract name
    identified_calls: HashMap<Address, String>,
    sources: ContractsSourcesDump,
}

#[derive(Serialize)]
struct ContractsSourcesDump {
    ids_by_name: HashMap<String, Vec<String>>,
    sources: HashMap<String, HashMap<String, ContractSourceDetailsDump>>,
}

#[derive(Serialize)]
struct ContractSourceDetailsDump {
    source_code: String,
    contract_bytecode: ContractBytecodeSome,
    source_path: Option<PathBuf>,
}

fn to_executions_dump(debugger_context: &DebuggerContext) -> ExecutionsDump {
    ExecutionsDump {
        calls: debugger_context.debug_arena.iter().map(to_call_dump).collect(),
        pc_ic_maps: debugger_context
            .pc_ic_maps
            .iter()
            .map(|(k, v)| (k.clone(), to_pc_ic_map_dump(v)))
            .collect(),
    }
}

fn to_call_dump(call: &DebugNodeFlat) -> CallDump {
    CallDump {
        address: call.address,
        kind: call.kind,
        steps: call.steps.iter().map(|step| to_step_dump(step.clone())).collect(),
    }
}

fn to_step_dump(step: DebugStep) -> StepDump {
    StepDump {
        stack: step.stack,
        memory: step.memory,
        calldata: step.calldata,
        returndata: step.returndata,
        instruction: foundry_evm_core::debug::Instruction::OpCode(step.instruction),
        push_bytes: Bytes::from(step.push_bytes.to_vec()),
        pc: step.pc,
        total_gas_used: step.total_gas_used,
    }
}

fn to_pc_ic_map_dump(pc_ic_map: &(PcIcMap, PcIcMap)) -> PcIcMapDump {
    let mut create_code_map = HashMap::new();
    for (k, v) in pc_ic_map.0.inner.iter() {
        create_code_map.insert(*k, *v);
    }

    let mut runtime_code_map = HashMap::new();
    for (k, v) in pc_ic_map.1.inner.iter() {
        runtime_code_map.insert(*k, *v);
    }

    PcIcMapDump { create_code_map, runtime_code_map }
}

fn to_contracts_dump(debugger_context: &DebuggerContext) -> Result<ContractsDump> {
    let identified_calls = debugger_context.identified_contracts.clone();
    let sources = to_contracts_sources_dump(&debugger_context.contracts_sources)?;
    Ok(ContractsDump { identified_calls, sources })
}

fn get_source_path_from_id(
    source_id: &u32,
    contract_name: &String,
    contracts_sources: &ContractSources,
) -> Result<String> {
    Ok(contracts_sources
        .sources
        .get(source_id)
        .unwrap()
        .get(contract_name)
        .unwrap()
        .2
        .as_ref()
        .unwrap()
        .clone()
        .to_string_lossy()
        .into_owned())
}

fn to_contracts_sources_dump(contracts_sources: &ContractSources) -> Result<ContractsSourcesDump> {
    let ids_by_name: HashMap<String, Vec<String>> = contracts_sources
        .ids_by_name
        .clone()
        .iter_mut()
        .map(|(name, ids)| {
            let vec_str: Vec<String> = ids
                .iter_mut()
                .map(|id| {
                    let source_path: String =
                        get_source_path_from_id(id, name, contracts_sources).unwrap();
                    source_path
                })
                .collect();
            (name.to_owned(), vec_str)
        })
        .collect();
    // println!("contracts_sources.ids_by_name {:#?}", contracts_sources.ids_by_name);
    // println!("ids_by_name {:#?}", ids_by_name);
    // println!("contracts_sources.sources {:#?}", contracts_sources.sources.keys());
    let mut sources = HashMap::new();
    contracts_sources.sources.iter().for_each(|(_id, m)| {
        m.iter().for_each(|(contract_name, (source_code, contract_bytecode, source_path))| {
            let contract = ContractSourceDetailsDump {
                source_code: source_code.clone(),
                contract_bytecode: contract_bytecode.clone(),
                source_path: source_path.clone(),
            };
            let source_path_key = source_path.clone().unwrap().to_string_lossy().into_owned();
            if sources.contains_key(&source_path_key) {
                let value: &mut HashMap<String, ContractSourceDetailsDump> =
                    sources.get_mut(&source_path_key).unwrap();
                value.insert(contract_name.clone(), contract);
            } else {
                let mut value = HashMap::new();
                value.insert(contract_name.clone(), contract);
                sources.insert(source_path_key, value);
            }
        })
    });
    let contract = ContractsSourcesDump { ids_by_name, sources };
    Ok(contract)
}
