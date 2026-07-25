using System;
using System.Xml;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class SendingOfflineChecks
{
	internal TypErrStr SendDoc()
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		TypErrKsef typErrKsef = All.l.CheckForSend();
		if (typErrKsef.errCode > 0)
		{
			result.errCode = typErrKsef.errCode;
			result.errStr = typErrKsef.errStr;
			return result;
		}
		XmlDocument xmlDocument = new XmlDocument();
		string idCancel = "";
		if (Operators.CompareString(typErrKsef.ReturnTyp, "1", TextCompare: false) == 0)
		{
			try
			{
				xmlDocument.LoadXml(typErrKsef.ReturnXML);
				idCancel = xmlDocument.SelectSingleNode("rq/dat/c/webcheck/@idcancel").Value;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				idCancel = "";
				ProjectData.ClearProjectError();
			}
		}
		string returnStr = All.l.ReturnOpenShift().ReturnStr;
		TypErrMacNumDat typErrMacNumDat = All.ReturnLastCheckMac(returnStr);
		if (typErrMacNumDat.errCode > 0)
		{
			result.errCode = typErrMacNumDat.errCode;
			result.errStr = typErrMacNumDat.errStr;
			return result;
		}
		string newValue = "<MAC ID='" + typErrKsef.ReturnNumber + "'>" + typErrMacNumDat.returnStr + "</MAC>";
		typErrKsef.ReturnXML = typErrKsef.ReturnXML.Replace("mmmaaaccc", newValue);
		if (!All.d.VerifyXML(typErrKsef.ReturnXML))
		{
			result.errCode = 39;
			result.errStr = "Ошибка при формировании офлайн XML чека для отправки.";
			All.Lg.SaveTextToLog("SendDoc", typErrKsef.ReturnXML, "Ошибка при проверке структуры XML офлайн чека");
			return result;
		}
		string text = All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\SendOffline.xml";
		All.SaveToFileText(text, typErrKsef.ReturnXML);
		TypErrOperKeyPass typErrOperKeyPass = All.l.OperatorKeyPass(returnStr);
		if (typErrOperKeyPass.errCode > 0)
		{
			result.errCode = typErrOperKeyPass.errCode;
			result.errStr = typErrOperKeyPass.errStr;
			return result;
		}
		TypErr typErr = All.SF.SignatureFile(typErrOperKeyPass.KeyFile.Trim(), typErrOperKeyPass.Pass.Trim(), text);
		if (typErr.errCode > 0)
		{
			result.errCode = typErr.errCode;
			result.errStr = typErr.errStr;
			result.ReturnStr = "";
			return result;
		}
		XmlDocument xmlDocument2 = new XmlDocument();
		xmlDocument2.LoadXml(typErrKsef.ReturnXML.ToLower());
		string innerText = xmlDocument2.GetElementsByTagName("ts")[0].InnerText;
		long dd = 0L;
		if (Versioned.IsNumeric(innerText))
		{
			dd = Convert.ToInt64(innerText);
		}
		text += ".p7s";
		TypErrSubmit typErrSubmit = default(SubmitPtr).SubmitCheck(text, typErrKsef.ReturnLocalNumber, TypToPROTO(typErrKsef.ReturnTyp), dd, idCancel, typErrKsef.ReturnNumber);
		if (typErrSubmit.errCode > 0)
		{
			result.errCode = typErrSubmit.errCode;
			result.errStr = typErrSubmit.errStr + "  Status: " + typErrSubmit.returnStatus + "  Msg: " + typErrSubmit.returnStr + "   ";
			return result;
		}
		TypErr typErr2 = default(TypErr);
		typErr2.errCode = 0;
		typErr2.errStr = "";
		if (Operators.CompareString(typErrKsef.ReturnTyp, "9", TextCompare: false) == 0)
		{
			typErr2 = All.l.UPDATEksef(typErrKsef.ReturnID, "offline", "3");
			if (typErr2.errCode > 0)
			{
				result.errCode = typErr2.errCode;
				result.errStr = typErr2.errStr;
				return result;
			}
		}
		else
		{
			typErr2 = All.l.UPDATEksef(typErrKsef.ReturnID, "offline", "1");
			if (typErr2.errCode > 0)
			{
				result.errCode = typErr2.errCode;
				result.errStr = typErr2.errStr;
				return result;
			}
		}
		typErr2 = All.l.UPDATEksef(typErrKsef.ReturnID, "signedanswerfromficscal", typErrSubmit.returnStr);
		if (typErr2.errCode > 0)
		{
			result.errCode = typErr2.errCode;
			result.errStr = typErr2.errStr;
			return result;
		}
		typErr2 = All.l.UPDATEksef(typErrKsef.ReturnID, "checksigned", typErrKsef.ReturnXML);
		if (typErr2.errCode > 0)
		{
			result.errCode = typErr2.errCode;
			result.errStr = typErr2.errStr;
			return result;
		}
		result.ReturnStr = "_CheckID=" + typErrKsef.ReturnNumber.Replace("_", "`");
		return result;
	}

	private byte TypToPROTO(string t)
	{
		return Conversions.ToInteger(t) switch
		{
			0 => 1, 
			1 => 1, 
			3 => 1, 
			4 => 1, 
			80 => 2, 
			_ => 3, 
		};
	}

	internal TypErrStr CloseOfflineDocOffline()
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		long num = All.DateOfflineClose();
		int idCh = checked(Conversions.ToInteger(All.l.MaxID("ksef").ReturnStr) + 1);
		string text = EndOfflineXML(num, idCh);
		string returnStr = All.l.ReturnOpenShift().ReturnStr;
		TypErrMacNumDat typErrMacNumDat = All.ReturnLastCheckMac(returnStr);
		if (typErrMacNumDat.errCode > 0)
		{
			result.errCode = typErrMacNumDat.errCode;
			result.errStr = typErrMacNumDat.errStr;
			return result;
		}
		All.OfflineNum = "";
		TypErrStr typErrStr = new NumbersOfflineUse().OfflineID();
		if (typErrStr.errCode > 0)
		{
			result.errCode = typErrStr.errCode;
			result.errStr = typErrStr.errStr;
			return result;
		}
		All.OfflineNum = typErrStr.ReturnStr;
		string newValue = "<MAC ID='" + All.OfflineNum + "'>" + typErrMacNumDat.returnStr + "</MAC>";
		text = text.Replace("mmmaaaccc", newValue);
		if (!All.d.VerifyXML(text))
		{
			result.errCode = 39;
			result.errStr = "Ошибка при формировании XML документа для отправки.";
			All.Lg.SaveTextToLog("CloseOfflineDoc", text, "Ошибка при проверке структуры XML документа");
			return result;
		}
		string text2 = All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\SendOffline.xml";
		All.SaveToFileText(text2, text);
		TypErrOperKeyPass typErrOperKeyPass = All.l.OperatorKeyPass(returnStr);
		if (typErrOperKeyPass.errCode > 0)
		{
			result.errCode = typErrOperKeyPass.errCode;
			result.errStr = typErrOperKeyPass.errStr;
			return result;
		}
		TypErr typErr = All.SF.SignatureFile(typErrOperKeyPass.KeyFile.Trim(), typErrOperKeyPass.Pass.Trim(), text2);
		if (typErr.errCode > 0)
		{
			result.errCode = typErr.errCode;
			result.errStr = typErr.errStr;
			result.ReturnStr = "";
			return result;
		}
		string pathFile = text2 + ".p7s";
		TypErrSubmit typErrSubmit = default(SubmitPtr).SubmitCheck(pathFile, idCh.ToString(), 3, num, "", All.OfflineNum);
		if (typErrSubmit.errCode > 0)
		{
			result.errCode = typErrSubmit.errCode;
			result.errStr = typErrSubmit.errStr + "  Status: " + typErrSubmit.returnStatus + "  Msg: " + typErrSubmit.returnStr + "   ";
			return result;
		}
		TypErr typErr2 = All.l.SaveXMLcheckOffline("Service", text, text, typErrSubmit.returnStr, typErrSubmit.returnNumber, "10", text2);
		if (typErrOperKeyPass.errCode > 0)
		{
			result.errCode = typErr2.errCode;
			result.errStr = typErr2.errStr;
			return result;
		}
		TypErr typErr3 = All.l.CloseOfflineKsef();
		if (typErr3.errCode > 0)
		{
			result.errCode = typErr3.errCode;
			result.errStr = typErr3.errStr;
			return result;
		}
		result.ReturnStr = "_CheckID=" + typErrSubmit.returnNumber.Replace("_", "`");
		return result;
	}

	internal TypErrStr CloseOfflineDoc()
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		long num = All.СurrentCompDate();
		int idCh = checked(Conversions.ToInteger(All.l.MaxID("ksef").ReturnStr) + 1);
		string text = EndOfflineXML(num, idCh);
		string returnStr = All.l.ReturnOpenShift().ReturnStr;
		TypErrMacNumDat typErrMacNumDat = All.ReturnLastCheckMac(returnStr);
		if (typErrMacNumDat.errCode > 0)
		{
			result.errCode = typErrMacNumDat.errCode;
			result.errStr = typErrMacNumDat.errStr;
			return result;
		}
		string newValue = "<MAC>" + typErrMacNumDat.returnStr + "</MAC>";
		text = text.Replace("mmmaaaccc", newValue);
		if (!All.d.VerifyXML(text))
		{
			result.errCode = 39;
			result.errStr = "Ошибка при формировании XML документа для отправки.";
			All.Lg.SaveTextToLog("CloseOfflineDoc", text, "Ошибка при проверке структуры XML документа");
			return result;
		}
		string text2 = All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\SendOffline.xml";
		All.SaveToFileText(text2, text);
		TypErrOperKeyPass typErrOperKeyPass = All.l.OperatorKeyPass(returnStr);
		if (typErrOperKeyPass.errCode > 0)
		{
			result.errCode = typErrOperKeyPass.errCode;
			result.errStr = typErrOperKeyPass.errStr;
			return result;
		}
		TypErr typErr = All.SF.SignatureFile(typErrOperKeyPass.KeyFile.Trim(), typErrOperKeyPass.Pass.Trim(), text2);
		if (typErr.errCode > 0)
		{
			result.errCode = typErr.errCode;
			result.errStr = typErr.errStr;
			result.ReturnStr = "";
			return result;
		}
		string pathFile = text2 + ".p7s";
		TypErrSubmit typErrSubmit = default(SubmitPtr).SubmitCheck(pathFile, "Локальный номер в смене", 3, num);
		if (typErrSubmit.errCode > 0)
		{
			result.errCode = typErrSubmit.errCode;
			result.errStr = typErrSubmit.errStr + "  Status: " + typErrSubmit.returnStatus + "  Msg: " + typErrSubmit.returnStr + "   ";
			return result;
		}
		TypErr typErr2 = All.l.SaveXMLcheck("Service", text, text, typErrSubmit.returnStr, typErrSubmit.returnNumber, "10", "0.00", text2);
		if (typErr2.errCode > 0)
		{
			result.errCode = typErr2.errCode;
			result.errStr = typErr2.errStr;
			return result;
		}
		TypErr typErr3 = All.l.CloseOfflineKsef();
		if (typErr3.errCode > 0)
		{
			result.errCode = typErr3.errCode;
			result.errStr = typErr3.errStr;
			return result;
		}
		result.ReturnStr = "_CheckID=" + typErrSubmit.returnNumber.Replace("_", "`");
		return result;
	}

	private string EndOfflineXML(long CCD, int idCh)
	{
		string text = "110";
		if (All.A.verAPI == 1)
		{
			text = "10";
		}
		return string.Concat(string.Concat(string.Concat(string.Concat(string.Concat("<?xml version='1.0' encoding='windows-1251'?><RQ V ='1'>" + "<DAT  FN='" + All.A.FN + "'", " TN='ПН ", All.A.TIN, "' ZN=''"), " DI='", idCh.ToString(), "' V='1'>"), "<C T='", text, "'></C>"), "<TS>", CCD.ToString(), "</TS></DAT>"), "mmmaaaccc</RQ>");
	}
}
