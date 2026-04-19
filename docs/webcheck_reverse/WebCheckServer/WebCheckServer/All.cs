using System;
using System.IO;
using System.Xml;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;
using WebCheck1;
using WebCheck10;
using WebCheck11;
using WebCheck12;
using WebCheck13;
using WebCheck14;
using WebCheck15;
using WebCheck16;
using WebCheck17;
using WebCheck18;
using WebCheck19;
using WebCheck2;
using WebCheck20;
using WebCheck21;
using WebCheck22;
using WebCheck23;
using WebCheck24;
using WebCheck25;
using WebCheck26;
using WebCheck27;
using WebCheck28;
using WebCheck29;
using WebCheck3;
using WebCheck30;
using WebCheck4;
using WebCheck5;
using WebCheck6;
using WebCheck7;
using WebCheck8;
using WebCheck9;

namespace WebCheckServer;

[StandardModule]
internal sealed class All
{
	internal const string VersionDll = "1.3.5";

	internal const string Proga = "WebCheck";

	internal const int RRR = 333;

	internal const int clRRR = 327;

	internal static WebCheck1.ClassFiscal W1 = new WebCheck1.ClassFiscal();

	internal static WebCheck2.ClassFiscal W2 = new WebCheck2.ClassFiscal();

	internal static WebCheck3.ClassFiscal W3 = new WebCheck3.ClassFiscal();

	internal static WebCheck4.ClassFiscal W4 = new WebCheck4.ClassFiscal();

	internal static WebCheck5.ClassFiscal W5 = new WebCheck5.ClassFiscal();

	internal static WebCheck6.ClassFiscal W6 = new WebCheck6.ClassFiscal();

	internal static WebCheck7.ClassFiscal W7 = new WebCheck7.ClassFiscal();

	internal static WebCheck8.ClassFiscal W8 = new WebCheck8.ClassFiscal();

	internal static WebCheck9.ClassFiscal W9 = new WebCheck9.ClassFiscal();

	internal static WebCheck10.ClassFiscal W10 = new WebCheck10.ClassFiscal();

	internal static WebCheck11.ClassFiscal W11 = new WebCheck11.ClassFiscal();

	internal static WebCheck12.ClassFiscal W12 = new WebCheck12.ClassFiscal();

	internal static WebCheck13.ClassFiscal W13 = new WebCheck13.ClassFiscal();

	internal static WebCheck14.ClassFiscal W14 = new WebCheck14.ClassFiscal();

	internal static WebCheck15.ClassFiscal W15 = new WebCheck15.ClassFiscal();

	internal static WebCheck16.ClassFiscal W16 = new WebCheck16.ClassFiscal();

	internal static WebCheck17.ClassFiscal W17 = new WebCheck17.ClassFiscal();

	internal static WebCheck18.ClassFiscal W18 = new WebCheck18.ClassFiscal();

	internal static WebCheck19.ClassFiscal W19 = new WebCheck19.ClassFiscal();

	internal static WebCheck20.ClassFiscal W20 = new WebCheck20.ClassFiscal();

	internal static WebCheck21.ClassFiscal W21 = new WebCheck21.ClassFiscal();

	internal static WebCheck22.ClassFiscal W22 = new WebCheck22.ClassFiscal();

	internal static WebCheck23.ClassFiscal W23 = new WebCheck23.ClassFiscal();

	internal static WebCheck24.ClassFiscal W24 = new WebCheck24.ClassFiscal();

	internal static WebCheck25.ClassFiscal W25 = new WebCheck25.ClassFiscal();

	internal static WebCheck26.ClassFiscal W26 = new WebCheck26.ClassFiscal();

	internal static WebCheck27.ClassFiscal W27 = new WebCheck27.ClassFiscal();

	internal static WebCheck28.ClassFiscal W28 = new WebCheck28.ClassFiscal();

	internal static WebCheck29.ClassFiscal W29 = new WebCheck29.ClassFiscal();

	internal static WebCheck30.ClassFiscal W30 = new WebCheck30.ClassFiscal();

	internal static bool PointRegion;

	internal static LogSaveText Log = new LogSaveText();

	internal static TypReply[] ReP = new TypReply[334];

	internal static IniHGB F = new IniHGB(MyDoc() + "\\WebCheck\\settings.ini");

	internal static int kuN = 0;

	internal static bool KuL = false;

	internal static TypStart A;

	internal static string NumberTaxVk;

	internal static int xS;

	internal static int xF;

	internal static int zS;

	internal static int zF;

	internal static int sS;

	internal static int sF;

	internal static int gS;

	internal static int gF;

	internal static int DayT = 0;

	private static void ZeroLog()
	{
		int day = DateTime.Now.Day;
		if (day != DayT)
		{
			DayT = day;
			xS = 0;
			xF = 0;
			zS = 0;
			zF = 0;
			sS = 0;
			sF = 0;
			gS = 0;
			gF = 0;
		}
	}

	internal static TypErrStr TestFN(string fnXML, string knotXML = "", bool TransactionControl = true)
	{
		TypErrStr typErrStr = default(TypErrStr);
		typErrStr.errCode = 0;
		typErrStr.errStr = "";
		typErrStr.ReturnStr = "";
		typErrStr.FN = "";
		int year = DateTime.Now.Year;
		Log.PathFile = MyDoc() + "\\WebCheck\\Temp\\log_" + year + ".txt";
		ZeroLog();
		typErrStr = GetParametrToString(fnXML, "fn", knotXML);
		if (typErrStr.errCode > 0)
		{
			return typErrStr;
		}
		if (Operators.CompareString(F.StringGetFn(typErrStr.FN, "Path"), "", false) == 0)
		{
			typErrStr.errCode = 9999;
			typErrStr.errStr = "Ошибка! Такого ФН нет или его параметр Path пустой.";
			return typErrStr;
		}
		if (Operators.CompareString(F.StringGetFn(typErrStr.FN, "On"), "0", false) == 0)
		{
			typErrStr.errCode = 9999;
			typErrStr.errStr = "Ошибка! ФН заблокирован.";
			return typErrStr;
		}
		TestUnique(typErrStr.FN);
		if (TransactionControl)
		{
			if (Operators.CompareString(W1.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W2.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W3.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W4.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W5.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W6.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W7.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W8.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W9.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W10.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W11.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W12.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W13.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W14.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W15.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W16.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W17.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W18.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W19.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W20.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W21.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W22.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W23.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W24.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W25.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W26.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W27.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W28.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W29.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
			if (Operators.CompareString(W30.StatusFN(), typErrStr.FN, false) == 0)
			{
				typErrStr.errCode = 9999;
				typErrStr.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
				return typErrStr;
			}
		}
		return typErrStr;
	}

	internal static TypErrStr TestFNuniquely(string FN)
	{
		TypErrStr result = default(TypErrStr);
		result.ReturnStr = "";
		result.errCode = 0;
		result.errStr = "";
		result.FN = "";
		if (Operators.CompareString(W1.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W2.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W3.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W4.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W5.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W6.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W7.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W8.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W9.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W10.StatusFN(), result.FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W11.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W12.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W13.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W14.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W15.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W16.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W17.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W18.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W19.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W20.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W21.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W22.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W23.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W24.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W25.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W26.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W27.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W28.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W29.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		if (Operators.CompareString(W30.StatusFN(), FN, false) == 0)
		{
			result.errCode = 9999;
			result.errStr = "Ошибка. Данный ФН имеет отрытую транзакцию с сервером налоговой.";
			return result;
		}
		return result;
	}

	internal static string XMLRiplyOk(string fnS)
	{
		if (Operators.CompareString(fnS.Trim(), "", false) == 0)
		{
			fnS = "Unknown";
		}
		return "<OutputParameters><Parameters Err='0' FN='" + fnS + "'/></OutputParameters>";
	}

	internal static string XMLRiplyOkInitialization(string fnS)
	{
		if (Operators.CompareString(fnS.Trim(), "", false) == 0)
		{
			fnS = "Unknown";
		}
		return "<OutputParameters><Parameters Err='0' TIN=\" FN=\" RegionSeparator=\" version=Server1.3.5' license=\" OfflineCount=\" Offline=\" OfflinePool=\"/></OutputParameters>";
	}

	internal static string XMLRiplyErr(string fnS, string errS)
	{
		if (Operators.CompareString(fnS.Trim(), "", false) == 0)
		{
			fnS = "Unknown";
		}
		if (Operators.CompareString(errS.Trim(), "", false) == 0)
		{
			errS = "Указан ошибочный фискальный номер";
		}
		return "<OutputParameters><Parameters Err='" + errS + "' FN='" + fnS + "'/></OutputParameters>";
	}

	internal static bool ReplyRemember(string FnS, string RepCom, string RepXML = "")
	{
		if (FnS.Length != 10)
		{
			Log.SaveTextToLog(FnS, "ReplyRemember", RepCom, "Ошибка фискального номера");
			return false;
		}
		if (!Versioned.IsNumeric((object)FnS))
		{
			Log.SaveTextToLog(FnS, "ReplyRemember", RepCom, "Ошибка фискального номера");
			return false;
		}
		NumberTaxVk = LastNumberTax(RepXML);
		int num = 0;
		checked
		{
			do
			{
				if (ReP[num].ClearControl > 0)
				{
					ReP[num].ClearControl--;
					if (ReP[num].ClearControl < 1)
					{
						ReP[num].FN = "";
						ReP[num].ReplyErr = "";
						ReP[num].ReplyPrt = "";
						ReP[num].ClearControl = 0;
					}
				}
				num++;
			}
			while (num <= 333);
			num = 0;
			do
			{
				if (Operators.CompareString(ReP[num].FN, FnS, false) == 0)
				{
					ReP[num].ReplyErr = RepCom;
					ReP[num].ReplyPrt = RepXML;
					ReP[num].ClearControl = 327;
					return true;
				}
				num++;
			}
			while (num <= 333);
			num = 0;
			do
			{
				if (Operators.CompareString(ReP[num].FN, "", false) == 0)
				{
					ReP[num].FN = FnS;
					ReP[num].ReplyErr = RepCom;
					ReP[num].ReplyPrt = RepXML;
					ReP[num].ClearControl = 327;
					return true;
				}
				num++;
			}
			while (num <= 333);
			Log.SaveTextToLog(FnS, "ReplyRemember", RepCom, "Стек памяти ответов переполнен");
			return false;
		}
	}

	internal static TypReply ReplyRemember(string FnS)
	{
		TypReply result = default(TypReply);
		result.FN = FnS;
		result.ReplyErr = XMLRiplyErr(FnS, "");
		result.ReplyPrt = "";
		if (FnS.Length != 10)
		{
			return result;
		}
		if (!Versioned.IsNumeric((object)FnS))
		{
			return result;
		}
		int num = 0;
		do
		{
			if (Operators.CompareString(ReP[num].FN, FnS, false) == 0)
			{
				result.ReplyErr = ReP[num].ReplyErr;
				result.ReplyPrt = ReP[num].ReplyPrt;
				ReP[num].FN = "";
				ReP[num].ReplyErr = "";
				ReP[num].ReplyPrt = "";
				ReP[num].ClearControl = 0;
				return result;
			}
			num = checked(num + 1);
		}
		while (num <= 333);
		result.FN = FnS;
		result.ReplyErr = XMLRiplyErr(FnS, "Поиск ответа. Такого фискального номера нет");
		result.ReplyPrt = "";
		return result;
	}

	internal static string MyDoc()
	{
		return Environment.GetFolderPath(Environment.SpecialFolder.CommonApplicationData);
	}

	internal static void NewFolder()
	{
		if (!Directory.Exists(MyDoc() + "\\WebCheck\\"))
		{
			Directory.CreateDirectory(MyDoc() + "\\WebCheck\\");
		}
		if (!Directory.Exists(MyDoc() + "\\WebCheck\\DB\\"))
		{
			Directory.CreateDirectory(MyDoc() + "\\WebCheck\\DB\\");
		}
		if (!Directory.Exists(MyDoc() + "\\WebCheck\\Keys\\"))
		{
			Directory.CreateDirectory(MyDoc() + "\\WebCheck\\Keys\\");
		}
		if (!Directory.Exists(MyDoc() + "\\WebCheck\\Temp\\"))
		{
			Directory.CreateDirectory(MyDoc() + "\\WebCheck\\Temp\\");
		}
		if (!Directory.Exists(MyDoc() + "\\WebCheck\\Archive\\"))
		{
			Directory.CreateDirectory(MyDoc() + "\\WebCheck\\Archive\\");
		}
		if (!Directory.Exists(MyDoc() + "\\WebCheck\\Lic\\"))
		{
			Directory.CreateDirectory(MyDoc() + "\\WebCheck\\Lic\\");
		}
		if (!Directory.Exists(MyDoc() + "\\WebCheck\\Temp\\All\\"))
		{
			Directory.CreateDirectory(MyDoc() + "\\WebCheck\\Temp\\All\\");
		}
		if (!Directory.Exists(MyDoc() + "\\WebCheck\\Logo\\"))
		{
			Directory.CreateDirectory(MyDoc() + "\\WebCheck\\Logo\\");
		}
	}

	internal static TypErrStr TestRegion()
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		result.FN = "";
		if (Versioned.IsNumeric((object)"1.2"))
		{
			PointRegion = true;
			result.ReturnStr = "RegionSeparator=point";
		}
		else
		{
			PointRegion = false;
			result.ReturnStr = "RegionSeparator=comma";
		}
		return result;
	}

	internal static float StToSi(string strP)
	{
		float result;
		try
		{
			if (!PointRegion)
			{
				strP = Strings.Replace(strP, ".", ",", 1, -1, (CompareMethod)0);
				result = Conversions.ToSingle(strP);
			}
			else
			{
				strP = Strings.Replace(strP, ",", ".", 1, -1, (CompareMethod)0);
				result = Conversions.ToSingle(strP);
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = 0f;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal static string SiToSt(float sinC)
	{
		string result;
		try
		{
			result = Strings.Replace(sinC.ToString(), ",", ".", 1, -1, (CompareMethod)0);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal static double StrToDouble(string strD)
	{
		double result;
		try
		{
			string text = strD;
			text = (PointRegion ? Strings.Replace(text, ",", ".", 1, -1, (CompareMethod)0) : Strings.Replace(text, ".", ",", 1, -1, (CompareMethod)0));
			result = ((!Versioned.IsNumeric((object)text)) ? 0.0 : Conversions.ToDouble(text));
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = 0.0;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal static string Bablo(string sBablo, bool sDot = true)
	{
		string result;
		try
		{
			string text = sBablo.Trim();
			text = (A.PointRegion ? Strings.Replace(text, ",", ".", 1, -1, (CompareMethod)0) : Strings.Replace(text, ".", ",", 1, -1, (CompareMethod)0));
			if (Versioned.IsNumeric((object)text))
			{
				text = Strings.FormatNumber((object)text, 9, (TriState)(-2), (TriState)(-2), (TriState)(-2));
				text = Strings.FormatNumber((object)text, 8, (TriState)(-2), (TriState)(-2), (TriState)(-2));
				text = Strings.FormatNumber((object)text, 7, (TriState)(-2), (TriState)(-2), (TriState)(-2));
				text = Strings.FormatNumber((object)text, 6, (TriState)(-2), (TriState)(-2), (TriState)(-2));
				text = Strings.FormatNumber((object)text, 5, (TriState)(-2), (TriState)(-2), (TriState)(-2));
				text = Strings.FormatNumber((object)text, 4, (TriState)(-2), (TriState)(-2), (TriState)(-2));
				text = Strings.FormatNumber((object)text, 3, (TriState)(-2), (TriState)(-2), (TriState)(-2));
				text = Strings.FormatNumber((object)text, 2, (TriState)(-2), (TriState)(-2), (TriState)(-2));
				text = Strings.Replace(text, ",", ".", 1, -1, (CompareMethod)0);
			}
			else
			{
				text = "0.00";
			}
			text = NoSpaceString(text);
			result = ((!sDot) ? Strings.Replace(text, ".", "", 1, -1, (CompareMethod)0) : text);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			if (sDot)
			{
				result = "0.00";
				ProjectData.ClearProjectError();
			}
			else
			{
				result = "000";
				ProjectData.ClearProjectError();
			}
		}
		return result;
	}

	internal static string NoSpaceString(string StrSpace)
	{
		string text = "";
		string text2 = "";
		checked
		{
			int num = StrSpace.Length - 1;
			for (int i = 0; i <= num; i++)
			{
				text2 = Conversions.ToString(StrSpace[i]);
				text += text2.Trim();
			}
			return text;
		}
	}

	public static TypErrStr GetParametrToString(string sXML, string name, string knot = "InputParameters/Parameters", bool RegUpLow = false)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		result.FN = "";
		sXML = sXML.Trim();
		name = name.Trim();
		knot = knot.Trim();
		try
		{
			XmlDocument xmlDocument = new XmlDocument();
			if (!RegUpLow)
			{
				sXML = sXML.ToLower();
				name = name.ToLower();
				knot = knot.ToLower();
			}
			xmlDocument.LoadXml(sXML);
			result.ReturnStr = xmlDocument.SelectSingleNode("/" + knot + "/@" + name).Value;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.ReturnStr = "";
			result.errCode = 9999;
			result.errStr = "Ошибка! Неверный формат XML.";
			ProjectData.ClearProjectError();
		}
		result.FN = result.ReturnStr;
		return result;
	}

	internal static string LastNumberTax(string xmlPr)
	{
		TypErrStr parametrToString = GetParametrToString(xmlPr, "CheckID", "OutputParameters/Parameters", RegUpLow: true);
		if (parametrToString.errCode > 0)
		{
			return "";
		}
		return parametrToString.ReturnStr;
	}

	public static bool VerifyXML(string strXMLv, string nameXMLfile = "")
	{
		bool result;
		try
		{
			XmlDocument xmlDocument = new XmlDocument();
			xmlDocument.LoadXml(strXMLv);
			if (nameXMLfile.Length > 0)
			{
				string filename = MyDoc() + "\\WebCheck\\Temp\\" + nameXMLfile + ".xml";
				xmlDocument.Save(filename);
			}
			result = true;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private static bool TestUnique(string eFN)
	{
		if (kuN > 1)
		{
			if (!KuL)
			{
				Log.SaveTextToLog(eFN, "Помилка унікальності серверної DLL", "Увага! Створюється більше одного екземпляра класу!");
				KuL = true;
			}
			return false;
		}
		return true;
	}
}
