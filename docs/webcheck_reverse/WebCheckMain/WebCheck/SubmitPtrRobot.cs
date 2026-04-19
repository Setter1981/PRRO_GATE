using System;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;
using TaxGrpc;

namespace WebCheck;

internal class SubmitPtrRobot
{
	private readonly string FN;

	private readonly string TIN;

	private readonly string FiscalMode;

	private readonly string Connection;

	private readonly string KeyOp;

	private readonly string PassOp;

	private readonly LogSaveTextRobot Log;

	public SubmitPtrRobot(string fnS, string fmS, string conS, string KeyS, string PassS, string TinS)
	{
		FN = "";
		TIN = "";
		FiscalMode = "";
		Connection = "";
		FN = fnS;
		TIN = TinS;
		FiscalMode = fmS;
		Connection = conS;
		KeyOp = KeyS.Trim();
		PassOp = PassS.Trim();
		Log = new LogSaveTextRobot(Connection, FN);
		Log.PathFile = All.MyDoc() + "\\WebCheck\\Temp\\" + FN + "\\log.txt";
		Log.FNlog = FN;
	}

	internal TypErrSubmit SubmitCheck(string pathFile, string numbCheck, byte typCheck, long dd, string idCancel = "", string idOffline = "", string TypeLC = "", string diLC = "", string ccdLC = "")
	{
		TypErrSubmit typErrSubmit = default(TypErrSubmit);
		typErrSubmit.errCode = 0;
		typErrSubmit.errStr = "";
		typErrSubmit.returnNumber = "";
		typErrSubmit.returnStatus = -81;
		typErrSubmit.returnStr = "";
		typErrSubmit.returnData = "";
		string text = "";
		int number;
		if (Versioned.IsNumeric((object)numbCheck))
		{
			number = Conversions.ToInteger(numbCheck);
		}
		else
		{
			string lastLocalCheckNumbern = new SQLrobot(Connection).ReturnLocalCheckNumberShift().LastLocalCheckNumbern;
			if (Versioned.IsNumeric((object)lastLocalCheckNumbern))
			{
				number = Conversions.ToInteger(lastLocalCheckNumbern);
				number = ((number > 0) ? checked(number + 1) : 0);
			}
			else
			{
				number = 0;
			}
		}
		TypErrSubmit result;
		try
		{
			int verAPI = All.A.verAPI;
			string text2 = dd.ToString();
			int num = Conversions.ToInteger(Conversions.ToString(text2[0]) + Conversions.ToString(text2[1]) + Conversions.ToString(text2[2]) + Conversions.ToString(text2[3]));
			int num2 = Conversions.ToInteger(Conversions.ToString(text2[4]) + Conversions.ToString(text2[5]));
			if (All.A.verAPI == 2 && num < 2022 && num2 < 10)
			{
				All.A.verAPI = 1;
			}
			Client client = new Client();
			client.Open(FiscalMode);
			Answer answer = client.Check(All.A.verAPI, pathFile, FN, dd, number, typCheck, idCancel, idOffline, 81000);
			All.A.verAPI = verAPI;
			Application.DoEvents();
			typErrSubmit.returnStatus = answer.Status;
			typErrSubmit.returnNumber = answer.Id.Replace("_", "`");
			typErrSubmit.returnStr = answer.IdSign;
			typErrSubmit.returnData = answer.IdData;
			text = answer.Message.Replace("_", " ");
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			typErrSubmit.errCode = 25;
			typErrSubmit.errStr = "Ошибка отправки чека для фискализации";
			result = typErrSubmit;
			ProjectData.ClearProjectError();
			goto IL_03dc;
		}
		if (typErrSubmit.returnStatus == 0)
		{
			if (typErrSubmit.returnStr.Trim().Length < 3)
			{
				typErrSubmit.errCode = 100;
				typErrSubmit.errStr = "Нет ответного чека с подписью налоговой";
				result = typErrSubmit;
				goto IL_03dc;
			}
			typErrSubmit.errCode = 32;
			typErrSubmit.errStr = ErrNumToString(typErrSubmit.returnStatus);
		}
		else if (typErrSubmit.returnStatus == -1)
		{
			typErrSubmit.returnStr = ErrNumToString(typErrSubmit.returnStatus);
			typErrSubmit.errCode = 67;
			typErrSubmit.errStr = "Сервер податкової не прийняв чек!";
			if (text.Trim().Length > 0)
			{
				ref string errStr = ref typErrSubmit.errStr;
				errStr = errStr + " (Відповідь сервера: " + text + ")";
			}
		}
		else if (typErrSubmit.returnStatus == -5)
		{
			typErrSubmit.returnStr = ErrNumToString(typErrSubmit.returnStatus);
			typErrSubmit.errCode = 25;
			typErrSubmit.errStr = "ТИП ПОСЛЕДНЕГО ЧЕКА: " + TypeLC;
			if (text.Trim().Length > 0)
			{
				ref string errStr2 = ref typErrSubmit.errStr;
				errStr2 = errStr2 + " (" + text + ")";
			}
			if (Operators.CompareString(TypeLC.Trim(), "110", false) == 0)
			{
				FixOffline(diLC, ccdLC);
			}
		}
		else if (typErrSubmit.returnStatus != 1)
		{
			typErrSubmit.returnStr = ErrNumToString(typErrSubmit.returnStatus);
			typErrSubmit.errCode = 25;
			typErrSubmit.errStr = "Сервер налоговой не принял чек";
			if (text.Trim().Length > 0)
			{
				ref string errStr3 = ref typErrSubmit.errStr;
				errStr3 = errStr3 + " (" + text + ")";
			}
		}
		else if (Operators.CompareString(typErrSubmit.returnNumber.Trim(), "", false) == 0)
		{
			typErrSubmit.errCode = 25;
			typErrSubmit.errStr = "Ошибка отправки чека";
		}
		result = typErrSubmit;
		goto IL_03dc;
		IL_03dc:
		return result;
	}

	private void FixOffline(string diLC, string cddLC)
	{
		string infa = XMLOfflineOnRobot(diLC, cddLC);
		SQLrobot sQLrobot = new SQLrobot(Connection);
		string returnStr = sQLrobot.IDksefType3().ReturnStr;
		if (Conversions.ToInteger(returnStr) > 0)
		{
			sQLrobot.UPDATEksef(returnStr, "checksigned", infa);
			NumbersOfflineUseRobot numbersOfflineUseRobot = new NumbersOfflineUseRobot(Connection, FN);
			string returnStr2 = numbersOfflineUseRobot.OfflineID().ReturnStr;
			if (Operators.CompareString(returnStr2.Trim(), "", false) == 0)
			{
				Log.SaveTextToLog("FixOffline", "ВНИМАНИЕ! КРИТИЧЕСКАЯ СИТУАЦИЯ", "Закончились офлайн номера");
				return;
			}
			returnStr2 = Strings.Replace(returnStr2.Trim(), "`", "_", 1, -1, (CompareMethod)0);
			sQLrobot.UPDATEksef(returnStr, "checkidficscal", returnStr2);
			sQLrobot.UPDATEksef(returnStr, "offline", "2");
			numbersOfflineUseRobot.MarkFNS(returnStr2);
		}
	}

	internal string XMLOfflineOnRobot(string IDch, string CCD)
	{
		string text = "109";
		if (All.A.verAPI == 1)
		{
			text = "9";
		}
		return string.Concat(string.Concat(string.Concat(string.Concat(string.Concat("<RQ V ='1'>" + "<DAT  FN='" + All.A.FN + "'", " TN='ПН ", All.A.INN, "' ZN=''"), " DI='", IDch, "' V='1'>"), "<C T='", text, "'></C>"), "<TS>", CCD, "</TS></DAT>"), "mmmaaaccc</RQ>");
	}

	internal TypErrSubmit LastCheck(string pathFile)
	{
		TypErrSubmit typErrSubmit = default(TypErrSubmit);
		typErrSubmit.errCode = 0;
		typErrSubmit.errStr = "";
		typErrSubmit.returnNumber = "";
		typErrSubmit.returnStatus = -81;
		typErrSubmit.returnStr = "";
		typErrSubmit.returnData = "";
		TypErrSubmit result;
		try
		{
			Client client = new Client();
			client.Open(FiscalMode);
			Answer answer = client.CheckLast(pathFile, 9000);
			Application.DoEvents();
			typErrSubmit.returnStatus = answer.Status;
			typErrSubmit.returnNumber = answer.Id.Replace("_", "`");
			typErrSubmit.returnStr = answer.IdSign;
			typErrSubmit.returnData = answer.IdData;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			typErrSubmit.errCode = 25;
			typErrSubmit.errStr = "Помилка надсилання запиту на останній чек.";
			result = typErrSubmit;
			ProjectData.ClearProjectError();
			goto IL_01a8;
		}
		if (typErrSubmit.returnStatus == 0)
		{
			typErrSubmit.errCode = 32;
			typErrSubmit.errStr = ErrNumToString(typErrSubmit.returnStatus);
			result = typErrSubmit;
		}
		else
		{
			if (typErrSubmit.returnStatus == -1)
			{
				typErrSubmit.returnStr = ErrNumToString(typErrSubmit.returnStatus);
				typErrSubmit.errCode = 67;
				typErrSubmit.errStr = "Під час отримання останнього чека з податкової виникла помилка підпису.";
			}
			else if (typErrSubmit.returnStatus != 1)
			{
				typErrSubmit.returnStr = ErrNumToString(typErrSubmit.returnStatus);
				typErrSubmit.errCode = 31;
				typErrSubmit.errStr = "Помилка отримання останнього чека з податкової.";
			}
			else if (Operators.CompareString(typErrSubmit.returnData.Trim(), "", false) == 0)
			{
				typErrSubmit.errCode = 0;
				typErrSubmit.errStr = "";
				All.Lg.SaveTextToLog("LastCheck", "Отправлен запрос для получения последнего чека", "Ответ = ПустаяСтрока");
			}
			result = typErrSubmit;
		}
		goto IL_01a8;
		IL_01a8:
		return result;
	}

	private string ErrNumToString(int errNum)
	{
		return errNum switch
		{
			0 => "немає звязку з сервером ДПС", 
			-1 => "помилка перевірки підпису", 
			-2 => "помилка перевірки РРО", 
			-3 => "помилка запису", 
			-4 => "загальна помилка", 
			-5 => "помилка типу посилки", 
			-6 => "нема Z-звіту за попередній день", 
			-7 => "невірний формат XML (структура, фіскальний номер)", 
			-8 => "невірний формат XML дата не відповідає Check.date", 
			-9 => "невірний формат XML чеку", 
			-10 => "невірний формат Z-звіту", 
			-11 => "РРО заблокований, перевищено ліміт 168 годин офлайну", 
			-12 => "невірний хеш попереднього чеку", 
			-13 => "не зареєстровано ПРРО", 
			-14 => "не зареєстровано підписант", 
			-15 => "не відкрита зміна", 
			-16 => "невірний оффлайн ID", 
			_ => "не визначений", 
		};
	}
}
