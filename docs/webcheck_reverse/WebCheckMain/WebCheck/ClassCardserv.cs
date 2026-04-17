using System;
using System.ComponentModel;
using System.IO;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

public class ClassCardserv
{
	public ClassCardserv()
	{
		int year = DateTime.Now.Year;
		All.LgT.PathFile = All.MyDoc() + "\\WebCheck\\Cardserv\\log_" + year + ".txt";
		string path = All.MyDoc() + "\\WebCheck\\Cardserv\\log_" + checked(year - 2) + ".txt";
		if (File.Exists(path))
		{
			File.Delete(path);
		}
		All.B.CurrentStatus = "";
		All.B.ErrHelp = "";
		All.B.ErrCode = 0;
	}

	public bool Cardserv(string strXML)
	{
		//IL_0b7c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0b68: Unknown result type (might be due to invalid IL or missing references)
		All.LgT.SaveTextToLogCardserv("Incoming XML", strXML);
		TypCardserv e = default(TypCardserv);
		e.TRANSIDCHECK = All.d.GetParametrToString(strXML, "transidcheck").ReturnStr;
		e.TRANSID = All.d.GetParametrToString(strXML, "transid").ReturnStr;
		e.type = All.d.GetParametrToString(strXML, "type").ReturnStr;
		e.dest = All.d.GetParametrToString(strXML, "dest").ReturnStr;
		e.method = All.d.GetParametrToString(strXML, "method").ReturnStr;
		e.port = 2000;
		DestPortIP(ref e.dest, ref e.port);
		e.merchantId = All.d.GetParametrToString(strXML, "merchantid", "InputParameters/Parameters", RegUpLow: true).ReturnStr;
		if (Operators.CompareString(e.merchantId.Trim(), "", false) == 0)
		{
			e.merchantId = All.d.GetParametrToString(strXML, "merchantId", "InputParameters/Parameters", RegUpLow: true).ReturnStr;
		}
		if (Operators.CompareString(e.merchantId.Trim(), "", false) == 0)
		{
			e.merchantId = All.d.GetParametrToString(strXML, "MERCHANTID", "InputParameters/Parameters", RegUpLow: true).ReturnStr;
		}
		if (Operators.CompareString(e.merchantId.Trim(), "", false) == 0)
		{
			e.merchantId = All.d.GetParametrToString(strXML, "Merchantid", "InputParameters/Parameters", RegUpLow: true).ReturnStr;
		}
		if (Operators.CompareString(e.merchantId.Trim(), "", false) == 0)
		{
			e.merchantId = All.d.GetParametrToString(strXML, "MerchantId", "InputParameters/Parameters", RegUpLow: true).ReturnStr;
		}
		e.rrn = All.d.GetParametrToString(strXML, "rrn").ReturnStr;
		e.amount = All.d.GetParametrToString(strXML, "amount").ReturnStr;
		e.amountcash = All.d.GetParametrToString(strXML, "amountcash").ReturnStr;
		e.invoicenumber = All.d.GetParametrToString(strXML, "invoicenumber").ReturnStr;
		e.subMerchant = All.d.GetParametrToString(strXML, "submerchant").ReturnStr;
		if (Operators.CompareString(e.type, "ssijson", false) == 0)
		{
			e.type = "privat";
		}
		e.amountOfParts = All.d.GetParametrToString(strXML, "amountofparts").ReturnStr;
		if (e.type.Length > 5 && ((Operators.CompareString(e.method, "servicepbp", false) == 0) & (Operators.CompareString(e.type.Substring(0, 6), "privat", false) == 0)))
		{
			if (!Versioned.IsNumeric((object)e.amountOfParts))
			{
				All.B.ErrHelp = "Помилка XML";
				All.B.ErrCode = 103;
				All.B.CurrentStatus = "Err=" + All.B.ErrCode;
				All.LgT.SaveTextToLogCardserv("Cardserv", "Помилка XML", "Метод servicepbp должен содержать amountofparts");
				return false;
			}
			e.method = "purchase";
		}
		string returnStr = All.d.GetParametrToString(strXML, "agreementnum").ReturnStr;
		if (e.type.Length > 5 && ((Operators.CompareString(e.method, "servicerefpbp", false) == 0) & (Operators.CompareString(e.type.Substring(0, 6), "privat", false) == 0)))
		{
			if (Operators.CompareString(returnStr, "", false) == 0)
			{
				All.B.ErrHelp = "Помилка XML";
				All.B.ErrCode = 103;
				All.B.CurrentStatus = "Err=" + All.B.ErrCode;
				All.LgT.SaveTextToLogCardserv("Cardserv", "Помилка XML", "Метод servicepbp должен содержать amountofparts");
				return false;
			}
			e.method = "refund";
			e.rrn = returnStr;
			e.amountOfParts = "999";
		}
		if ((Operators.CompareString(All.MethodEnToUa(e.method), "", false) == 0) | (Operators.CompareString(e.type, "", false) == 0) | (Operators.CompareString(e.dest, "", false) == 0) | (Operators.CompareString(e.merchantId, "", false) == 0))
		{
			All.B.ErrHelp = "Помилка XML";
			All.B.ErrCode = 103;
			All.B.CurrentStatus = "Err=" + All.B.ErrCode;
			All.LgT.SaveTextToLogCardserv("Cardserv", "Помилка XML");
			return false;
		}
		if (Operators.CompareString(e.type, "bpos", false) == 0 && ((Operators.CompareString(e.merchantId.Trim(), "0", false) == 0) | (Operators.CompareString(e.merchantId.Trim(), "00", false) == 0) | (Operators.CompareString(e.merchantId.Trim(), "000", false) == 0)))
		{
			All.B.ErrHelp = "Помилка BPOS. Аргумент MerchantId не може бути 0";
			All.B.ErrCode = 103;
			All.B.CurrentStatus = "Err=" + All.B.ErrCode;
			All.LgT.SaveTextToLogCardserv("Cardserv", "Помилка BPOS. Аргумент MerchantId не може бути 0");
			return false;
		}
		if (Operators.CompareString(Conversions.ToString(e.dest[0]), "d", false) == 0)
		{
			e.Connect = 999;
		}
		else
		{
			e.Connect = 0;
			switch (e.type)
			{
			case "privat":
				if (Operators.CompareString(Conversions.ToString(e.dest[0]), "c", false) == 0)
				{
					e.Connect = 1;
				}
				else
				{
					e.Connect = 2;
				}
				break;
			case "privatold":
				if (Operators.CompareString(Conversions.ToString(e.dest[0]), "c", false) == 0)
				{
					e.Connect = 3;
				}
				else
				{
					e.Connect = 4;
				}
				break;
			case "bpos":
				if (Operators.CompareString(Conversions.ToString(e.dest[0]), "c", false) == 0)
				{
					e.Connect = 5;
				}
				else
				{
					e.Connect = 6;
				}
				break;
			case "posapi":
				e.Connect = 7;
				break;
			default:
				e.Connect = 0;
				break;
			}
		}
		if (e.Connect == 0)
		{
			All.B.ErrHelp = "Помилка XML";
			All.B.ErrCode = 103;
			All.B.CurrentStatus = "Err=" + All.B.ErrCode;
			All.LgT.SaveTextToLogCardserv("Cardserv", "Помилка XML");
			return false;
		}
		if ((Operators.CompareString(e.method, "purchase", false) == 0) | (Operators.CompareString(e.method, "refund", false) == 0) | (Operators.CompareString(e.method, "cashback", false) == 0))
		{
			if (!All.Money(e.amount))
			{
				All.B.ErrHelp = "Помилка XML";
				All.B.ErrCode = 103;
				All.B.CurrentStatus = "Err=" + All.B.ErrCode;
				All.LgT.SaveTextToLogCardserv("Cardserv", "Помилка XML");
				return false;
			}
			if (Operators.CompareString(e.method, "refund", false) == 0)
			{
				if (Operators.CompareString(e.rrn.Trim(), "", false) == 0)
				{
					All.B.ErrHelp = "Помилка XML";
					All.B.ErrCode = 103;
					All.B.CurrentStatus = "Err=" + All.B.ErrCode;
					All.LgT.SaveTextToLogCardserv("Cardserv", "Помилка XML");
					return false;
				}
			}
			else if (Operators.CompareString(e.method, "cashback", false) == 0 && !All.Money(e.amountcash))
			{
				All.B.ErrHelp = "Помилка XML";
				All.B.ErrCode = 103;
				All.B.CurrentStatus = "Err=" + All.B.ErrCode;
				All.LgT.SaveTextToLogCardserv("Cardserv", "Помилка XML");
				return false;
			}
		}
		_ = (Operators.CompareString(e.method, "audit", false) == 0) | (Operators.CompareString(e.method, "verify", false) == 0);
		if (Operators.CompareString(e.method, "withdrawal", false) == 0 && Operators.CompareString(e.invoicenumber.Trim(), "", false) == 0)
		{
			All.B.ErrHelp = "Помилка XML";
			All.B.ErrCode = 103;
			All.B.CurrentStatus = "Err=" + All.B.ErrCode;
			All.LgT.SaveTextToLogCardserv("Cardserv", "Помилка XML");
			return false;
		}
		if (Operators.CompareString(e.type, "bpos", false) == 0 && ((Operators.CompareString(e.method, "cashback", false) == 0) | (Operators.CompareString(e.method, "getmerchantlist", false) == 0)))
		{
			All.B.ErrHelp = "Метод " + e.method + " тимчасово не працює";
			All.B.ErrCode = 103;
			All.B.CurrentStatus = "Err=" + All.B.ErrCode;
			All.LgT.SaveTextToLogCardserv("Cardserv", "Помилка метода " + e.method);
			return false;
		}
		if (Operators.CompareString(e.type, "posapi", false) == 0 && Operators.CompareString(e.method, "getmerchantlist", false) == 0)
		{
			All.B.ErrHelp = "Метод " + e.method + " тимчасово не працює";
			All.B.ErrCode = 103;
			All.B.CurrentStatus = "Err=" + All.B.ErrCode;
			All.LgT.SaveTextToLogCardserv("Cardserv", "Помилка метода " + e.method);
			return false;
		}
		if (Operators.CompareString(e.type, "posapi", false) == 0)
		{
			FormTerminalA formTerminalA = new FormTerminalA(e);
			((Form)formTerminalA).ShowDialog();
			((Component)(object)formTerminalA).Dispose();
		}
		else
		{
			FormTerminal formTerminal = new FormTerminal(e);
			((Form)formTerminal).ShowDialog();
			((Component)(object)formTerminal).Dispose();
		}
		All.B.ErrHelp = "";
		All.B.ErrCode = 0;
		return All.CardservTrue;
	}

	private void DestPortIP(ref string destIP, ref int portIP)
	{
		if (!Versioned.IsNumeric((object)destIP[0]))
		{
			return;
		}
		string text = destIP;
		int num = portIP;
		try
		{
			string[] array = destIP.Split(new char[1] { ':' });
			if (array.Length > 1)
			{
				destIP = array[0].Trim();
				array[1] = array[1].Trim();
				if (Versioned.IsNumeric((object)array[1]))
				{
					portIP = Conversions.ToInteger(array[1]);
				}
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			destIP = text;
			portIP = num;
			ProjectData.ClearProjectError();
		}
	}

	public string StatusBarXML()
	{
		TypErrStr parametrToXMLterminal = default(TypErrStr);
		parametrToXMLterminal.errCode = 0;
		parametrToXMLterminal.errStr = "";
		parametrToXMLterminal.ReturnStr = All.B.CurrentStatus;
		if (All.B.ErrCode > 0)
		{
			ref string returnStr = ref parametrToXMLterminal.ReturnStr;
			returnStr = returnStr + "_ErrHelp=" + All.B.ErrHelp + "_version=" + All.VersionDll();
		}
		parametrToXMLterminal = All.d.GetParametrToXMLterminal(parametrToXMLterminal.ReturnStr);
		if (Operators.CompareString(parametrToXMLterminal.ReturnStr.Trim(), "", false) == 0)
		{
			All.LgT.SaveTextToLogCardserv("StatusBarXML", "Status request", "Error! Empty string");
		}
		else
		{
			All.LgT.SaveTextToLogCardserv("StatusBarXML", "Status request", parametrToXMLterminal.ReturnStr);
		}
		All.iTA.WriteString("General", "POS_TRANS_ID", "");
		return parametrToXMLterminal.ReturnStr;
	}
}
