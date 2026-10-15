//! Text parsing is separate from domain transitions.
use game_api::*;
pub enum Input {
    Status,
    Run(SimDuration),
    Save(String),
    Load(String),
    Debug,
    Command(Command),
    Quit,
}
pub fn parse(line: &str) -> Result<Input> {
    let parts: Vec<_> = line.split_whitespace().collect();
    let number = |i: usize| {
        parts
            .get(i)
            .ok_or_else(|| Error::Rejected("missing argument".into()))?
            .parse::<u64>()
            .map_err(|_| Error::Rejected("expected an unsigned integer".into()))
    };
    match parts.as_slice() {
  ["status"]=>Ok(Input::Status),["debug"]=>Ok(Input::Debug),["quit"]=>Ok(Input::Quit),
  ["run",_]=>Ok(Input::Run(SimDuration(number(1)?.checked_mul(1000).ok_or(Error::Overflow)?))),
  ["move",_]=>Ok(Input::Command(Command::Move{destination:AreaId(number(1)?)})),
  ["eat",_]=>Ok(Input::Command(Command::Eat{inventory:InventoryId(number(1)?)})),["rest"]=>Ok(Input::Command(Command::Rest)),
  ["look"]=>Ok(Input::Command(Command::ObserveSurroundings)),
  ["observe-patch",_]=>Ok(Input::Command(Command::ObservePatch{patch:number(1)?})),
  ["tell",_,key]=>Ok(Input::Command(Command::Tell{recipient:PersonId(number(1)?),key:(*key).into()})),
  ["observe",_]=>Ok(Input::Command(Command::Observe{inventory:InventoryId(number(1)?)})),
  ["sow",_,_]=>Ok(Input::Command(Command::Sow{plot:PlotId(number(1)?),seed:InventoryId(number(2)?)})),
  ["harvest",_,_]=>Ok(Input::Command(Command::Harvest{plot:PlotId(number(1)?),destination:InventoryId(number(2)?)})),
  ["dismiss"]=>Ok(Input::Command(Command::Dismiss)),["cancel"]=>Ok(Input::Command(Command::Cancel)),
  ["consent",_,"yes"]=>Ok(Input::Command(Command::Consent{step:number(1)? as usize,accept:true})),
  ["consent",_,"no"]=>Ok(Input::Command(Command::Consent{step:number(1)? as usize,accept:false})),
  ["save",path]=>Ok(Input::Save((*path).into())),["load",path]=>Ok(Input::Load((*path).into())),
  ["command",..]=>Ok(Input::Command(serde_json::from_str(line.trim().strip_prefix("command").unwrap().trim()).map_err(|e|Error::Rejected(e.to_string()))?)),
  _=>Err(Error::Rejected("use status, run <seconds>, move <area>, eat <inventory>, rest, observe <inventory>, look, observe-patch <patch>, tell <person> <fact-key>, sow <plot> <inventory>, harvest <plot> <inventory>, cancel, consent <step> yes|no, dismiss, command <JSON>, save <path>, load <path>, debug, quit".into())),
 }
}
