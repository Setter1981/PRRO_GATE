using System.IO;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class CheckLastCheck
{
	internal int NumberLastCheck(string INNoperator)
	{
		if (All.l.OfflineTrue())
		{
			return 0;
		}
		All.MacTempOld = "";
		if (All.ReturnLastCheckMac(INNoperator, operINN: true).errCode > 0)
		{
			return 0;
		}
		string pathF = All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\000.xml";
		string xmlLast = LastCheckXml(pathF);
		return NumberLC(xmlLast);
	}

	internal TypErrStr NumberLastCheckTest(string INNoperator)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		if (All.l.OfflineTrue())
		{
			result.errCode = 50;
			result.errStr = "Включен офлайн режим";
			result.ReturnStr = "";
			return result;
		}
		All.MacTempOld = "";
		TypErrMacNumDat typErrMacNumDat = All.ReturnLastCheckMac(INNoperator, operINN: true);
		if (typErrMacNumDat.errCode > 0)
		{
			result.errCode = typErrMacNumDat.errCode;
			result.errStr = typErrMacNumDat.errStr;
			result.ReturnStr = typErrMacNumDat.returnStr;
			return result;
		}
		string pathF = All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\000.xml";
		string xmlLast = LastCheckXml(pathF);
		result.ReturnStr = NumberLC(xmlLast).ToString();
		return result;
	}

	private string LastCheckXml(string PathF)
	{
		if (!File.Exists(PathF))
		{
			return "";
		}
		string text = "";
		StreamReader streamReader = new StreamReader(PathF);
		do
		{
			text += streamReader.ReadLine();
		}
		while (!streamReader.EndOfStream);
		streamReader.Close();
		return text;
	}

	internal int NumberLC(string xmlLast)
	{
		string returnStr = All.d.GetParametrToString(xmlLast, "di", "rq/dat").ReturnStr;
		if (!Versioned.IsNumeric((object)returnStr))
		{
			return 0;
		}
		return Conversions.ToInteger(returnStr);
	}

	internal TypErrMacNumDatDi LastCheckAllInfa()
	{
		TypErrMacNumDatDi result = default(TypErrMacNumDatDi);
		result.errCode = 0;
		result.errStr = "";
		result.returnStr = "";
		result.returnData = "";
		result.returnDI = 0;
		result.returnNumber = "";
		result.returnType = "";
		result.returnDate = "";
		if (All.l.OfflineTrue())
		{
			result.errCode = 50;
			result.errStr = "Включен офлайн режим";
			result.returnStr = "";
			return result;
		}
		OperatorsAll operatorsAll = new OperatorsAll();
		if (operatorsAll.Operators > 0)
		{
			string numberShift = operatorsAll.get_Seller(4, 1);
			All.MacTempOld = "";
			TypErrMacNumDat typErrMacNumDat = All.ReturnLastCheckMac(numberShift, operINN: true);
			if (typErrMacNumDat.errCode > 0)
			{
				result.errCode = typErrMacNumDat.errCode;
				result.errStr = typErrMacNumDat.errStr;
				result.returnStr = typErrMacNumDat.returnStr;
				return result;
			}
			result.returnData = typErrMacNumDat.returnData;
			result.returnNumber = typErrMacNumDat.returnNumber;
			result.returnStr = typErrMacNumDat.returnStr;
			string pathF = All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\000.xml";
			string xmlLast = LastCheckXml(pathF);
			result.returnDI = NumberLC(xmlLast);
			return result;
		}
		result.errCode = 1014;
		result.errStr = "Ошибка. Нет информации об операторах.";
		result.returnStr = "";
		return result;
	}
}
