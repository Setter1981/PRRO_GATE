using System;
using System.IO;
using System.Text;
using System.Threading;
using System.Windows.Forms;
using System.Xml;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.FileIO;

namespace WebCheck;

internal class SendingOfflineChecksRobot
{
	private readonly string Connection;

	private string KeyOp;

	private string PassOp;

	private readonly string FN;

	private readonly string TIN;

	private readonly string FiscalWWW;

	private readonly SQLrobot LR;

	private readonly IniHGB Blok;

	private readonly LogSaveTextRobot Log;

	public SendingOfflineChecksRobot(string ConnectionS, string KeyS, string PassS, string FNS, string fiscalS, string TinS)
	{
		Blok = new IniHGB(All.MyDoc() + "\\WebCheck\\settings.ini");
		Connection = ConnectionS.Trim();
		KeyOp = KeyS.Trim();
		PassOp = PassS.Trim();
		FN = FNS.Trim();
		FiscalWWW = fiscalS;
		TIN = TinS;
		LR = new SQLrobot(Connection);
		Log = new LogSaveTextRobot(Connection, FN);
		Log.PathFile = All.MyDoc() + "\\WebCheck\\Temp\\" + FN + "\\log.txt";
		Log.FNlog = FN;
	}

	internal TypErrStr SendDoc()
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		LR.LastOfflineDate();
		TypErrKsef typErrKsef = LR.CheckForSend();
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
		string returnShift = typErrKsef.ReturnShift;
		TypErrOperKeyPass typErrOperKeyPass = LR.OperatorKeyPass(returnShift);
		if (typErrOperKeyPass.errCode > 0)
		{
			result.errCode = typErrOperKeyPass.errCode;
			result.errStr = typErrOperKeyPass.errStr;
			return result;
		}
		KeyOp = typErrOperKeyPass.KeyFile;
		PassOp = typErrOperKeyPass.Pass;
		TypErrMacNumDatDi typErrMacNumDatDi = LastCheckAllInfa();
		if (typErrMacNumDatDi.errCode > 0)
		{
			result.errCode = typErrMacNumDatDi.errCode;
			result.errStr = typErrMacNumDatDi.errStr;
			return result;
		}
		checked
		{
			if (Operators.CompareString(typErrKsef.ReturnTyp.Trim(), "9", TextCompare: false) == 0 && Operators.CompareString(typErrKsef.ReturnID.Trim(), typErrMacNumDatDi.returnDI.ToString().Trim(), TextCompare: false) == 0 && Operators.CompareString(typErrKsef.ReturnNumber.Trim(), typErrMacNumDatDi.returnNumber.Trim(), TextCompare: false) != 0)
			{
				int num = (Versioned.IsNumeric(typErrKsef.ReturnID.Trim()) ? (Conversions.ToInteger(typErrKsef.ReturnID.Trim()) + 1) : 0);
				TypErr typErr = default(TypErr);
				typErr.errCode = 0;
				typErr.errStr = "";
				int num2 = 1;
				do
				{
					if (LR.UPDATEksef(num.ToString(), "offline", "1").errCode == 0)
					{
						new LogSaveTextRobot(Connection, FN).SaveTextToLog("SendDoc Внимание!", "Офлайн запись в таблице ksef ID " + num, "получила статус 1 - отправлена, без реальной отправки");
						break;
					}
					Thread.Sleep(500);
					num2++;
				}
				while (num2 <= 108);
			}
			if (Operators.CompareString(typErrKsef.ReturnNumber.Replace("`", "_"), typErrMacNumDatDi.returnNumber.Replace("`", "_"), TextCompare: false) == 0)
			{
				Log.SaveTextToLog("Номера отправляемого чека сопадают с номером последнего чека", typErrKsef.ReturnNumber.Replace("`", "_"), typErrMacNumDatDi.returnNumber.Replace("`", "_"));
				TypErr typErr2 = default(TypErr);
				typErr2.errCode = 0;
				typErr2.errStr = "";
				int num3;
				if (Operators.CompareString(typErrKsef.ReturnTyp, "9", TextCompare: false) == 0)
				{
					num3 = 1;
					do
					{
						typErr2 = LR.UPDATEksef(typErrKsef.ReturnID, "offline", "3");
						if (typErr2.errCode == 0)
						{
							break;
						}
						Thread.Sleep(500);
						num3++;
					}
					while (num3 <= 108);
					if (typErr2.errCode > 0)
					{
						result.errCode = typErr2.errCode;
						result.errStr = typErr2.errStr;
						return result;
					}
				}
				else
				{
					num3 = 1;
					do
					{
						typErr2 = LR.UPDATEksef(typErrKsef.ReturnID, "offline", "1");
						if (typErr2.errCode == 0)
						{
							break;
						}
						Thread.Sleep(500);
						num3++;
					}
					while (num3 <= 108);
					if (typErr2.errCode > 0)
					{
						result.errCode = typErr2.errCode;
						result.errStr = typErr2.errStr;
						return result;
					}
				}
				if (Operators.CompareString(typErrKsef.ReturnTyp, "80", TextCompare: false) != 0)
				{
					typErrMacNumDatDi.returnData = "";
				}
				num3 = 1;
				do
				{
					typErr2 = LR.UPDATEksef(typErrKsef.ReturnID, "signedanswerfromficscal", typErrMacNumDatDi.returnData);
					if (typErr2.errCode == 0)
					{
						break;
					}
					Thread.Sleep(500);
					num3++;
				}
				while (num3 <= 108);
				if (typErr2.errCode > 0)
				{
					result.errCode = typErr2.errCode;
					result.errStr = typErr2.errStr;
					return result;
				}
				num3 = 1;
				do
				{
					typErr2 = LR.UPDATEksef(typErrKsef.ReturnID, "checksigned", typErrKsef.ReturnXML);
					if (typErr2.errCode == 0)
					{
						break;
					}
					Thread.Sleep(500);
					num3++;
				}
				while (num3 <= 108);
				if (typErr2.errCode > 0)
				{
					result.errCode = typErr2.errCode;
					result.errStr = typErr2.errStr;
					return result;
				}
				result.ReturnStr = "_CheckID=" + typErrKsef.ReturnNumber.Replace("_", "`");
				return result;
			}
			string newValue = "<MAC ID='" + typErrKsef.ReturnNumber + "'>" + typErrMacNumDatDi.returnStr + "</MAC>";
			typErrKsef.ReturnXML = typErrKsef.ReturnXML.Replace("mmmaaaccc", newValue);
			if (!All.d.VerifyXML(typErrKsef.ReturnXML))
			{
				result.errCode = 39;
				result.errStr = "Ошибка при формировании офлайн XML чека для отправки.";
				return result;
			}
			string text = All.MyDoc() + "\\WebCheck\\Temp\\" + FN + "\\SendOffline.xml";
			All.SaveToFileText(text, typErrKsef.ReturnXML);
			TypErr typErr3 = All.SF.SignatureFile(KeyOp, PassOp, text);
			if (typErr3.errCode > 0)
			{
				result.errCode = typErr3.errCode;
				result.errStr = typErr3.errStr;
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
			TypErrSubmit typErrSubmit = new SubmitPtrRobot(FN, FiscalWWW, Connection, KeyOp, PassOp, TIN).SubmitCheck(text, typErrKsef.ReturnLocalNumber, TypToPROTO(typErrKsef.ReturnTyp), dd, idCancel, typErrKsef.ReturnNumber, typErrMacNumDatDi.returnType, typErrMacNumDatDi.returnDI.ToString(), typErrMacNumDatDi.returnDate);
			if (typErrSubmit.errCode > 0)
			{
				result.errCode = typErrSubmit.errCode;
				result.errStr = typErrSubmit.errStr + "  Status: " + typErrSubmit.returnStatus + "  Msg: " + typErrSubmit.returnStr + "   ";
				return result;
			}
			TypErr typErr4 = default(TypErr);
			typErr4.errCode = 0;
			typErr4.errStr = "";
			int num4;
			if (Operators.CompareString(typErrKsef.ReturnTyp, "9", TextCompare: false) == 0)
			{
				num4 = 1;
				do
				{
					typErr4 = LR.UPDATEksef(typErrKsef.ReturnID, "offline", "3");
					if (typErr4.errCode == 0)
					{
						break;
					}
					Thread.Sleep(500);
					num4++;
				}
				while (num4 <= 108);
				if (typErr4.errCode > 0)
				{
					result.errCode = typErr4.errCode;
					result.errStr = typErr4.errStr;
					return result;
				}
			}
			else
			{
				num4 = 1;
				do
				{
					typErr4 = LR.UPDATEksef(typErrKsef.ReturnID, "offline", "1");
					if (typErr4.errCode == 0)
					{
						break;
					}
					Thread.Sleep(500);
					num4++;
				}
				while (num4 <= 108);
				if (typErr4.errCode > 0)
				{
					result.errCode = typErr4.errCode;
					result.errStr = typErr4.errStr;
					return result;
				}
			}
			if (Operators.CompareString(typErrKsef.ReturnTyp, "80", TextCompare: false) != 0)
			{
				typErrSubmit.returnStr = "";
			}
			num4 = 1;
			do
			{
				typErr4 = LR.UPDATEksef(typErrKsef.ReturnID, "signedanswerfromficscal", typErrSubmit.returnStr);
				if (typErr4.errCode == 0)
				{
					break;
				}
				Thread.Sleep(500);
				num4++;
			}
			while (num4 <= 108);
			if (typErr4.errCode > 0)
			{
				result.errCode = typErr4.errCode;
				result.errStr = typErr4.errStr;
				return result;
			}
			num4 = 1;
			do
			{
				typErr4 = LR.UPDATEksef(typErrKsef.ReturnID, "checksigned", typErrKsef.ReturnXML);
				if (typErr4.errCode == 0)
				{
					break;
				}
				Thread.Sleep(500);
				num4++;
			}
			while (num4 <= 108);
			if (typErr4.errCode > 0)
			{
				result.errCode = typErr4.errCode;
				result.errStr = typErr4.errStr;
				return result;
			}
			result.ReturnStr = "_CheckID=" + typErrKsef.ReturnNumber.Replace("_", "`");
			return result;
		}
	}

	private byte TypToPROTO(string t)
	{
		return Conversions.ToInteger(t) switch
		{
			0 => 1, 
			1 => 1, 
			3 => 1, 
			4 => 1, 
			-8 => 1, 
			80 => 2, 
			_ => 3, 
		};
	}

	internal TypErrStr CloseOfflineDoc()
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		long num = All.СurrentCompDate();
		int idCh = checked(Conversions.ToInteger(LR.MaxID("ksef").ReturnStr) + 1);
		string text = EndOfflineXML(num, idCh);
		TypErrKsef typErrKsef = LR.LastCheckForSend();
		if (typErrKsef.errCode > 0)
		{
			result.errCode = typErrKsef.errCode;
			result.errStr = typErrKsef.errStr;
			return result;
		}
		string returnShift = typErrKsef.ReturnShift;
		TypErrOperKeyPass typErrOperKeyPass = LR.OperatorKeyPass(returnShift);
		if (typErrOperKeyPass.errCode > 0)
		{
			result.errCode = typErrOperKeyPass.errCode;
			result.errStr = typErrOperKeyPass.errStr;
			return result;
		}
		KeyOp = typErrOperKeyPass.KeyFile;
		PassOp = typErrOperKeyPass.Pass;
		TypErrStr typErrStr = ReturnLastCheckMac(KeyOp, PassOp);
		if (typErrStr.errCode > 0)
		{
			result.errCode = typErrStr.errCode;
			result.errStr = typErrStr.errStr;
			return result;
		}
		new NumbersOfflineUseRobot(Connection, FN);
		string newValue = "<MAC>" + typErrStr.ReturnStr + "</MAC>";
		text = text.Replace("mmmaaaccc", newValue);
		if (!All.d.VerifyXML(text))
		{
			result.errCode = 39;
			result.errStr = "Ошибка при формировании XML документа для отправки.";
			return result;
		}
		string text2 = All.MyDoc() + "\\WebCheck\\Temp\\" + FN + "\\SendOffline.xml";
		All.SaveToFileText(text2, text);
		TypErr typErr = All.SF.SignatureFile(KeyOp, PassOp, text2);
		if (typErr.errCode > 0)
		{
			result.errCode = typErr.errCode;
			result.errStr = typErr.errStr;
			result.ReturnStr = "";
			return result;
		}
		string pathFile = text2 + ".p7s";
		TypErrSubmit typErrSubmit = new SubmitPtrRobot(FN, FiscalWWW, Connection, KeyOp, PassOp, TIN).SubmitCheck(pathFile, "Локальный номер в смене", 3, num);
		if (typErrSubmit.errCode > 0)
		{
			TypErrMacNumDatDi typErrMacNumDatDi = LastCheckAllInfa();
			if (typErrMacNumDatDi.errCode > 0)
			{
				result.errCode = typErrSubmit.errCode;
				result.errStr = typErrSubmit.errStr + "  Status: " + typErrSubmit.returnStatus + "  Msg: " + typErrSubmit.returnStr + "   ";
				return result;
			}
			int returnDI = typErrMacNumDatDi.returnDI;
			int num2 = NumberLC(text);
			if (returnDI <= 0)
			{
				result.errCode = typErrSubmit.errCode;
				result.errStr = typErrSubmit.errStr + "  Status: " + typErrSubmit.returnStatus + "  Msg: " + typErrSubmit.returnStr + "   ";
				return result;
			}
			if (returnDI != num2)
			{
				result.errCode = typErrSubmit.errCode;
				result.errStr = typErrSubmit.errStr + "  Status: " + typErrSubmit.returnStatus + "  Msg: " + typErrSubmit.returnStr + "   ";
				return result;
			}
			typErrSubmit.returnStr = typErrMacNumDatDi.returnStr;
			typErrSubmit.returnNumber = typErrMacNumDatDi.returnNumber;
		}
		TypErr typErr2 = LR.SaveXMLcheck("Service", text, text, typErrSubmit.returnStr, typErrSubmit.returnNumber, "10", "0.00", text2);
		if (typErr2.errCode > 0)
		{
			result.errCode = typErr2.errCode;
			result.errStr = typErr2.errStr;
			return result;
		}
		TypErr typErr3 = LR.CloseOfflineKsef();
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
		return string.Concat(string.Concat(string.Concat(string.Concat(string.Concat("<?xml version='1.0' encoding='windows-1251'?><RQ V ='1'>" + "<DAT  FN='" + FN + "'", " TN='ПН ", TIN, "' ZN=''"), " DI='", idCh.ToString(), "' V='1'>"), "<C T='", text, "'></C>"), "<TS>", CCD.ToString(), "</TS></DAT>"), "mmmaaaccc</RQ>");
	}

	internal TypErrStr ReturnLastCheckMac(string KeyFile, string Pass)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		SaveToFileTextFN();
		string text = All.MyDoc() + "\\WebCheck\\Temp\\" + FN + "\\FN.xml";
		TypErr typErr = All.SF.SignatureFile(KeyFile.Trim(), Pass.Trim(), text);
		if (typErr.errCode > 0)
		{
			result.errCode = typErr.errCode;
			result.errStr = typErr.errStr;
			return result;
		}
		text += ".p7s";
		TypErrSubmit typErrSubmit = new SubmitPtrRobot(FN, FiscalWWW, Connection, KeyOp, PassOp, TIN).LastCheck(text);
		if (typErrSubmit.errCode > 0)
		{
			result.errCode = typErrSubmit.errCode;
			result.errStr = typErrSubmit.errStr + "  Status: " + typErrSubmit.returnStatus + "  Msg: " + typErrSubmit.returnStr + "   ";
			return result;
		}
		if (Operators.CompareString(typErrSubmit.returnData.Trim(), "", TextCompare: false) == 0)
		{
			result.ReturnStr = "";
			return result;
		}
		string text2 = All.MyDoc() + "\\WebCheck\\Temp\\" + FN + "\\000.xml";
		result = All.SF.DecodCheck(typErrSubmit.returnData, text2);
		if (result.errCode > 0)
		{
			result.ReturnStr = "";
			return result;
		}
		result.ReturnStr = SHA.GenerateSHA256File(text2);
		return result;
	}

	private bool SaveToFileTextFN()
	{
		string text = All.MyDoc() + "\\WebCheck\\Temp\\" + FN + "\\FN.xml";
		string fN = FN;
		try
		{
			if (File.Exists(text))
			{
				FileSystem.DeleteFile(text);
			}
			if (File.Exists(text + ".p7s"))
			{
				FileSystem.DeleteFile(text + ".p7s");
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
		bool result;
		try
		{
			Stream stream = File.OpenWrite(text);
			StreamWriter streamWriter = new StreamWriter(stream, Encoding.GetEncoding(1251));
			streamWriter.Write(fN);
			streamWriter.Flush();
			streamWriter.Close();
			result = true;
		}
		catch (Exception ex3)
		{
			ProjectData.SetProjectError(ex3);
			Exception ex4 = ex3;
			result = false;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal TypErr NewBackupNumbers(int nZ = 0)
	{
		TypErr typErr = default(TypErr);
		typErr.errCode = 0;
		typErr.errStr = "";
		TypErr result;
		checked
		{
			if (Operators.CompareString(FN, "7000000512", TextCompare: false) == 0)
			{
				typErr.errCode = 36;
				typErr.errStr = "В демонстрационной версии нет офлайн режим";
				result = typErr;
			}
			else
			{
				NumbersOfflineUseRobot numbersOfflineUseRobot = new NumbersOfflineUseRobot(Connection, FN);
				int num = numbersOfflineUseRobot.CountNubmers();
				string text = Blok.StringGetFn(FN, "OfflineMax");
				int num2;
				if (Versioned.IsNumeric(text))
				{
					num2 = Conversions.ToInteger(text);
				}
				else
				{
					num2 = 500;
					Blok.StringWriteFN(FN, "OfflineMax", num2.ToString());
				}
				string text2 = Blok.StringGetFn(FN, "OfflineMin");
				int num3;
				if (Versioned.IsNumeric(text2))
				{
					num3 = Conversions.ToInteger(text2);
				}
				else
				{
					num3 = 50;
					Blok.StringWriteFN(FN, "OfflineMin", num3.ToString());
				}
				if (num2 > 2000)
				{
					num2 = 2000;
				}
				if (num3 < 0)
				{
					num3 = 0;
				}
				if (nZ < 1)
				{
					if (num > num3)
					{
						result = typErr;
						goto IL_04d8;
					}
					nZ = num2;
				}
				int num4 = Conversions.ToInteger(LR.MaxID("ksef").ReturnStr) + 1;
				long dd = All.СurrentCompDate();
				string text3 = StringXML(nZ, num4.ToString(), dd.ToString());
				All.MacTempOld = "";
				TypErrStr typErrStr = ReturnLastCheckMac(KeyOp, PassOp);
				if (typErrStr.errCode > 0)
				{
					typErr.errCode = typErrStr.errCode;
					typErr.errStr = typErrStr.errStr;
					result = typErr;
				}
				else
				{
					string newValue = "<MAC>" + typErrStr.ReturnStr + "</MAC>";
					text3 = text3.Replace("mmmaaaccc", newValue);
					string text4 = All.MyDoc() + "\\WebCheck\\Temp\\" + FN + "\\NewOfflineN.xml";
					All.SaveToFileText(text4, text3);
					TypErr typErr2 = All.SF.SignatureFile(KeyOp, PassOp, text4);
					if (typErr2.errCode > 0)
					{
						typErr.errCode = typErr2.errCode;
						typErr.errStr = typErr2.errStr;
						result = typErr;
					}
					else
					{
						string pathFile = text4;
						text4 += ".p7s";
						TypErrSubmit typErrSubmit = new SubmitPtrRobot(FN, FiscalWWW, Connection, KeyOp, PassOp, TIN).SubmitCheck(text4, num4.ToString(), 3, dd);
						if (typErrSubmit.errCode > 0)
						{
							typErr.errCode = typErrSubmit.errCode;
							typErr.errStr = typErrSubmit.errStr + "  Status: " + typErrSubmit.returnStatus + "  Msg: " + typErrSubmit.returnStr + "   ";
							result = typErr;
						}
						else
						{
							string pathFile2 = All.MyDoc() + "\\WebCheck\\Temp\\" + FN + "\\000.xml";
							TypErrStr typErrStr2 = All.SF.DecodCheck(typErrSubmit.returnData, pathFile2);
							Application.DoEvents();
							if (typErrStr2.errCode > 0)
							{
								typErr.errCode = typErrStr2.errCode;
								typErr.errStr = typErrStr2.errStr;
								result = typErr;
							}
							else
							{
								XmlDocument xmlDocument = new XmlDocument();
								XmlDocument xmlDocument2 = new XmlDocument();
								try
								{
									xmlDocument.LoadXml(typErrStr2.ReturnStr);
								}
								catch (Exception ex)
								{
									ProjectData.SetProjectError(ex);
									Exception ex2 = ex;
									typErr.errCode = 48;
									typErr.errStr = "Ошибка - формат XML ответа.";
									result = typErr;
									ProjectData.ClearProjectError();
									goto IL_04d8;
								}
								Application.DoEvents();
								numbersOfflineUseRobot.UPDATEfns();
								Application.DoEvents();
								XmlNodeList elementsByTagName = xmlDocument.GetElementsByTagName("ID");
								int num5 = elementsByTagName.Count - 1;
								for (int i = 0; i <= num5; i++)
								{
									xmlDocument2.LoadXml(elementsByTagName[i].OuterXml);
									string innerText = xmlDocument2.GetElementsByTagName("ID")[0].InnerText;
									numbersOfflineUseRobot.Add(innerText, num4.ToString());
									Application.DoEvents();
								}
								TypErr typErr3 = LR.SaveXMLcheck("Service", text3, text3, typErrSubmit.returnData, typErrSubmit.returnNumber, "12", "0.00", pathFile);
								if (typErr3.errCode > 0)
								{
									typErr.errCode = typErr3.errCode;
									typErr.errStr = typErr3.errStr;
									result = typErr;
								}
								else
								{
									string returnStr = LR.MaxID("ksef").ReturnStr;
									TypPrintChecks typPrintChecks = All.Rf.CheckXMLNumber(returnStr, SearchID: true);
									new PrintExportCheck().ExportToFile(typPrintChecks.ReturnStr, typPrintChecks.ReturnStrTaxN);
									result = typErr;
								}
							}
						}
					}
				}
			}
			goto IL_04d8;
		}
		IL_04d8:
		return result;
	}

	private string StringXML(int nZ, string di, string CCD)
	{
		string text = "112";
		if (All.A.verAPI == 1)
		{
			text = "12";
		}
		string text2 = "<RQ V = '1'>";
		text2 = text2 + "<DAT FN='" + FN + "' TN='" + TIN + "' ZN='' DI='" + di + "' V='1'>";
		text2 = text2 + "<C T='" + text + "'><H SIZE='" + nZ + "'></H></C>";
		return text2 + "<TS>" + CCD + "</TS></DAT>mmmaaaccc</RQ>";
	}

	private TypErrMacNumDatDi LastCheckAllInfa()
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
		OperatorsAllRobot operatorsAllRobot = new OperatorsAllRobot(Connection, FN);
		if (operatorsAllRobot.Operators > 0)
		{
			operatorsAllRobot.get_Seller(4, 1);
			All.MacTempOld = "";
			TypErrMacNumDat typErrMacNumDat = ReturnLastCheckMacRobot(KeyOp, PassOp);
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
			string pathF = All.MyDoc() + "\\WebCheck\\Temp\\" + FN + "\\000.xml";
			string text = LastCheckXml(pathF);
			result.returnDI = NumberLC(text);
			result.returnType = TypeLC(text);
			result.returnDate = DateLC(text);
			return result;
		}
		result.errCode = 1014;
		result.errStr = "Ошибка. Нет информации об операторах.";
		result.returnStr = "";
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

	private int NumberLC(string xmlLast)
	{
		int result;
		try
		{
			string returnStr = All.d.GetParametrToString(xmlLast, "di", "rq/dat").ReturnStr;
			result = (Versioned.IsNumeric(returnStr) ? Conversions.ToInteger(returnStr) : 0);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = 0;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private string TypeLC(string xmlLast)
	{
		string result;
		try
		{
			result = All.d.GetParametrToString(xmlLast, "t", "rq/dat/c").ReturnStr;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = "ошибка";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private string DateLC(string xmllast)
	{
		XmlDocument xmlDocument = new XmlDocument();
		xmlDocument.LoadXml(xmllast.ToLower());
		return xmlDocument.GetElementsByTagName("ts")[0].InnerText;
	}

	private TypErrMacNumDat ReturnLastCheckMacRobot(string KeyFile, string Pass)
	{
		TypErrMacNumDat result = default(TypErrMacNumDat);
		result.errCode = 0;
		result.errStr = "";
		result.returnStr = "";
		result.returnNumber = "";
		result.returnData = "";
		SaveToFileTextFN();
		string text = All.MyDoc() + "\\WebCheck\\Temp\\" + FN + "\\FN.xml";
		TypErr typErr = All.SF.SignatureFile(KeyFile.Trim(), Pass.Trim(), text);
		if (typErr.errCode > 0)
		{
			result.errCode = typErr.errCode;
			result.errStr = typErr.errStr;
			return result;
		}
		text += ".p7s";
		TypErrSubmit typErrSubmit = new SubmitPtrRobot(FN, FiscalWWW, Connection, KeyOp, PassOp, TIN).LastCheck(text);
		if (typErrSubmit.errCode > 0)
		{
			result.errCode = typErrSubmit.errCode;
			result.errStr = typErrSubmit.errStr + "  Status: " + typErrSubmit.returnStatus + "  Msg: " + typErrSubmit.returnStr + "   ";
			return result;
		}
		result.returnNumber = typErrSubmit.returnNumber;
		result.returnData = typErrSubmit.returnData;
		if (Operators.CompareString(typErrSubmit.returnData.Trim(), "", TextCompare: false) == 0)
		{
			result.returnStr = "";
			return result;
		}
		string text2 = All.MyDoc() + "\\WebCheck\\Temp\\" + FN + "\\000.xml";
		All.SF.DecodCheck(typErrSubmit.returnData, text2);
		if (result.errCode > 0)
		{
			result.returnStr = "";
			return result;
		}
		Application.DoEvents();
		result.returnStr = SHA.GenerateSHA256File(text2);
		return result;
	}
}
