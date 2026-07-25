using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

public class ForServer
{
	public int TypWork
	{
		get
		{
			return All.A.TypWork;
		}
		set
		{
			All.A.TypWork = value;
		}
	}

	public bool OffLine => All.l.OfflineTrue();

	public bool FormTimerShow => All.A.FormRobot;

	public int PullNumber
	{
		get
		{
			return All.A.PullY;
		}
		set
		{
			All.A.PullY = value;
		}
	}

	public bool RequestOfflineNumbers(string eMin, string eMax)
	{
		if (!All.A.Status)
		{
			return false;
		}
		if (!All.A.FullVersion)
		{
			return false;
		}
		if (All.l.OfflineTrue())
		{
			return false;
		}
		int num = new NumbersOfflineUse().CountNubmers();
		int num2 = ((!Versioned.IsNumeric(eMax)) ? 500 : Conversions.ToInteger(eMax));
		int num3 = ((!Versioned.IsNumeric(eMin)) ? 50 : Conversions.ToInteger(eMin));
		if (num2 > 2000)
		{
			num2 = 2000;
		}
		if (num3 < 0)
		{
			num3 = 0;
		}
		if (num <= num3)
		{
			return true;
		}
		return false;
	}

	public bool OperatorsEndDateToIni()
	{
		if (!All.A.Status)
		{
			return false;
		}
		new OperatorsAllRobot(All.A.Connection, All.A.FN);
		return true;
	}

	public string VkToComXML(string XMLvk)
	{
		return new Vk().XMLvkToCom(XMLvk);
	}

	public string GetDocumentsByShifts(int ShiftID)
	{
		if (!All.A.Status)
		{
			return "";
		}
		if (ShiftID < 0)
		{
			All.Lg.SaveTextToLog("GetDocumentsByShifts", "Номер смены не может быть меньше ноля");
			return "";
		}
		string text = "<Checks ShiftId='" + ShiftID + "' FN='" + All.A.FN + "'>";
		CheckShiftAll checkShiftAll = new CheckShiftAll(ShiftID);
		if (checkShiftAll.Checks < 1)
		{
			All.Lg.SaveTextToLog("GetDocumentsByShifts", "В этой смене нет чеков для отображения");
			return "";
		}
		int checks = checkShiftAll.Checks;
		for (int i = 1; i <= checks; i = checked(i + 1))
		{
			TypChecks typChecks = checkShiftAll.InfaChecks(i);
			text = text + "<Check NumFiscal='" + typChecks.NumberCheck + "'";
			text = text + " NumLocal='" + typChecks.IdCheck + "'";
			text = text + " DocDateTime='" + typChecks.DateCheck + "'";
			text = text + " CheckSum='" + typChecks.SumCheck + "'";
			text = text + " CheckDocType='" + typChecks.TypCheck + "'/>";
		}
		checkShiftAll = null;
		text += "</Checks>";
		if (All.d.VerifyXML(text))
		{
			return text;
		}
		All.Lg.SaveTextToLog("GetDocumentsByShifts", "Ошибка верификации XML", text);
		return "";
	}

	public string GetDocumentsByShiftsEX(int ShiftID, string FNs)
	{
		if (All.A.Status)
		{
			return "";
		}
		if (ShiftID < 0)
		{
			All.Lg.SaveTextToLog("GetDocumentsByShiftsEX", "Номер смены не может быть меньше ноля");
			return "";
		}
		string text = "<Checks ShiftId='" + ShiftID + "' FN='" + All.A.FN + "'>";
		CheckShiftAll checkShiftAll = new CheckShiftAll(ShiftID, FNs);
		if (checkShiftAll.Checks < 1)
		{
			All.Lg.SaveTextToLog("GetDocumentsByShiftsEX", "В этой смене нет чеков для отображения");
			return "";
		}
		int checks = checkShiftAll.Checks;
		for (int i = 1; i <= checks; i = checked(i + 1))
		{
			TypChecks typChecks = checkShiftAll.InfaChecks(i);
			text = text + "<Check NumFiscal='" + typChecks.NumberCheck + "'";
			text = text + " NumLocal='" + typChecks.IdCheck + "'";
			text = text + " DocDateTime='" + typChecks.DateCheck + "'";
			text = text + " CheckSum='" + typChecks.SumCheck + "'";
			text = text + " CheckDocType='" + typChecks.TypCheck + "'/>";
		}
		checkShiftAll = null;
		text += "</Checks>";
		if (All.d.VerifyXML(text))
		{
			return text;
		}
		All.Lg.SaveTextToLog("GetDocumentsByShiftsEX", "Ошибка верификации XML", text);
		return "";
	}

	public string GetShiftsDate(string ShiftMonth)
	{
		if (!All.A.Status)
		{
			return "";
		}
		string text = "";
		checked
		{
			if (ShiftMonth.Trim().Length < 7)
			{
				text = "<Shifts FN='" + All.A.FN + "'>";
				ShiftsAll shiftsAll = new ShiftsAll();
				if (shiftsAll.Shifts < 1)
				{
					All.Lg.SaveTextToLog("GetShiftsDate", "В отборе по данному запросу нет смен");
					return "";
				}
				int shifts = shiftsAll.Shifts;
				for (int i = 1; i <= shifts; i++)
				{
					text = text + "<Shift NumLocal='" + shiftsAll.get_InfaSheft(0, i) + "'";
					text = text + " ShiftStart='" + shiftsAll.get_InfaSheft(1, i) + "'";
					text = text + " ShfitEnd='" + shiftsAll.get_InfaSheft(2, i) + "'";
					text = text + " OperatorName='" + shiftsAll.get_InfaSheft(3, i) + "'";
					text = text + " OperatorId='" + shiftsAll.get_InfaSheft(5, i) + "'";
					text = text + " ChecksinShift='" + shiftsAll.get_InfaSheft(4, i) + "'/>";
				}
				shiftsAll = null;
			}
			else
			{
				string text2 = Conversions.ToString(ShiftMonth[0]) + Conversions.ToString(ShiftMonth[1]);
				string text3 = Conversions.ToString(ShiftMonth[3]) + Conversions.ToString(ShiftMonth[4]) + Conversions.ToString(ShiftMonth[5]) + Conversions.ToString(ShiftMonth[6]);
				text = "<Shifts ShiftMonth='" + text2 + "." + text3 + "' FN='" + All.A.FN + "'>";
				ShiftsAll shiftsAll2 = new ShiftsAll(text2, text3);
				if (shiftsAll2.Shifts < 1)
				{
					All.Lg.SaveTextToLog("GetShiftsDate", "В отборе по данному запросу нет смен");
					return "";
				}
				int shifts2 = shiftsAll2.Shifts;
				for (int j = 1; j <= shifts2; j++)
				{
					text = text + "<Shift NumLocal='" + shiftsAll2.get_InfaSheft(0, j) + "'";
					text = text + " ShiftStart='" + shiftsAll2.get_InfaSheft(1, j) + "'";
					text = text + " ShfitEnd='" + shiftsAll2.get_InfaSheft(2, j) + "'";
					text = text + " OperatorName='" + shiftsAll2.get_InfaSheft(3, j) + "'";
					text = text + " OperatorId='" + shiftsAll2.get_InfaSheft(5, j) + "'";
					text = text + " ChecksinShift='" + shiftsAll2.get_InfaSheft(4, j) + "'/>";
				}
				shiftsAll2 = null;
			}
			text += "</Shifts>";
			if (All.d.VerifyXML(text))
			{
				return text;
			}
			All.Lg.SaveTextToLog("GetShiftsDate", "Ошибка верификации XML", text);
			return "";
		}
	}

	public string GetShiftsDateEX(string ShiftMonth, string FNs)
	{
		if (All.A.Status)
		{
			return "";
		}
		string text = "";
		checked
		{
			if (ShiftMonth.Trim().Length < 7)
			{
				text = "<Shifts FN='" + All.A.FN + "'>";
				ShiftsAll shiftsAll = new ShiftsAll("", "", FNs);
				if (shiftsAll.Shifts < 1)
				{
					All.Lg.SaveTextToLog("GetShiftsDateEX", "В отборе по данному запросу нет смен");
					return "";
				}
				int shifts = shiftsAll.Shifts;
				for (int i = 1; i <= shifts; i++)
				{
					text = text + "<Shift NumLocal='" + shiftsAll.get_InfaSheft(0, i) + "'";
					text = text + " ShiftStart='" + shiftsAll.get_InfaSheft(1, i) + "'";
					text = text + " ShfitEnd='" + shiftsAll.get_InfaSheft(2, i) + "'";
					text = text + " OperatorName='" + shiftsAll.get_InfaSheft(3, i) + "'";
					text = text + " OperatorId='" + shiftsAll.get_InfaSheft(5, i) + "'";
					text = text + " ChecksinShift='" + shiftsAll.get_InfaSheft(4, i) + "'/>";
				}
				shiftsAll = null;
			}
			else
			{
				string text2 = Conversions.ToString(ShiftMonth[0]) + Conversions.ToString(ShiftMonth[1]);
				string text3 = Conversions.ToString(ShiftMonth[3]) + Conversions.ToString(ShiftMonth[4]) + Conversions.ToString(ShiftMonth[5]) + Conversions.ToString(ShiftMonth[6]);
				text = "<Shifts ShiftMonth='" + text2 + "." + text3 + "' FN='" + All.A.FN + "'>";
				ShiftsAll shiftsAll2 = new ShiftsAll(text2, text3, FNs);
				if (shiftsAll2.Shifts < 1)
				{
					All.Lg.SaveTextToLog("GetShiftsDateEX", "В отборе по данному запросу нет смен");
					return "";
				}
				int shifts2 = shiftsAll2.Shifts;
				for (int j = 1; j <= shifts2; j++)
				{
					text = text + "<Shift NumLocal='" + shiftsAll2.get_InfaSheft(0, j) + "'";
					text = text + " ShiftStart='" + shiftsAll2.get_InfaSheft(1, j) + "'";
					text = text + " ShfitEnd='" + shiftsAll2.get_InfaSheft(2, j) + "'";
					text = text + " OperatorName='" + shiftsAll2.get_InfaSheft(3, j) + "'";
					text = text + " OperatorId='" + shiftsAll2.get_InfaSheft(5, j) + "'";
					text = text + " ChecksinShift='" + shiftsAll2.get_InfaSheft(4, j) + "'/>";
				}
				shiftsAll2 = null;
			}
			text += "</Shifts>";
			if (All.d.VerifyXML(text))
			{
				return text;
			}
			All.Lg.SaveTextToLog("GetShiftsDateEX", "Ошибка верификации XML", text);
			return "";
		}
	}

	public string GetCashEX(string FNs)
	{
		if (All.A.Status)
		{
			return "";
		}
		All.A.FileN = All.f.StringGetFn(FNs, "Path");
		All.A.Connection = "Data Source=" + All.A.FileN + "; Version=3";
		string text = All.Bablo(All.Nal().ToString());
		All.A.Status = false;
		All.A.FileN = "";
		All.A.Connection = "";
		return "<OutputParameters><Parameters FN='" + FNs + "' CashBalance='" + text + "'/></OutputParameters>";
	}

	public string GetCheckByFiscalNumberEX(string TAXn, string FNs)
	{
		if (All.A.Status)
		{
			return "";
		}
		TypErrStr typErrStr = All.Rf.Reprt5(TAXn, 1, FNs);
		if (typErrStr.errCode > 0)
		{
			return "";
		}
		TypErrStr typErrStr2 = All.Rf.Reprt5(TAXn, 8, FNs);
		if (typErrStr2.errCode > 0)
		{
			typErrStr2.ReturnStr = "";
		}
		TypErrStr typErrStr3 = All.Rf.Reprt5(TAXn, 4, FNs);
		if (typErrStr3.errCode > 0)
		{
			typErrStr3.ReturnStr = "0";
		}
		string replacement = "<MAC ID='" + typErrStr3.ReturnStr + "'>" + typErrStr2.ReturnStr + "</MAC>";
		return Strings.Replace(typErrStr.ReturnStr, "mmmaaaccc", replacement);
	}

	public string GetShiftStatusEX(string FNs)
	{
		if (All.A.Status)
		{
			return "";
		}
		string returnStr = All.l.ReturnOpenShiftEX(FNs).ReturnStr;
		return "<OutputParameters><Parameters FN='" + FNs + "' ShiftNumber='" + returnStr + "'/></OutputParameters>";
	}

	public bool InitializationForAddPRRO(string strFN)
	{
		All.TestRegion();
		TypErrStr parametrToString = All.d.GetParametrToString(strFN, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			return false;
		}
		if (!Versioned.IsNumeric(parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.errCode = 3;
			parametrToString.errStr = "Неправильный формат Фискального Номера: " + parametrToString.ReturnStr;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			return false;
		}
		if (Operators.CompareString(All.f.StringGetFn(parametrToString.ReturnStr, "Path"), "", TextCompare: false) != 0)
		{
			parametrToString.errCode = 93;
			parametrToString.errStr = "Фискальнsq Номер " + parametrToString.ReturnStr + " уже есть в базе нет.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrCode = parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			return false;
		}
		if (All.A.Status)
		{
			parametrToString.errCode = 2;
			parametrToString.errStr = "Ранее был подключен Фискальный Номер: " + All.A.FN;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			return false;
		}
		All.A.FN = parametrToString.ReturnStr.Trim();
		All.NewFolderFn();
		All.Lg.PathFile = All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\log.txt";
		All.A.Status = true;
		All.A.CurrentStatus = "Err=0_FN=" + All.A.FN + "_version=" + All.VersionDll();
		All.A.ErrHelp = "";
		All.A.ErrCode = 0;
		All.Lg.SaveTextToLog("InitializationForAddPRRO", strFN, "Инициализация для создания нового PRO");
		return true;
	}
}
