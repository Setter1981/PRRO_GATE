using System;
using System.IO;
using System.Net;
using System.Xml;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.FileIO;

namespace WebCheck;

internal class InViber
{
	internal TypErr InTextViber(string nTax, string nPhone, int tSend)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		try
		{
			if (!All.A.Status)
			{
				result.errCode = 91;
				result.errStr = "Ошибка! Для отправки чека в вайбер необходима инициализация";
				return result;
			}
			if (!All.A.FullVersion)
			{
				result.errCode = 91;
				result.errStr = "Ошибка! Функция отправки чека в вайбер доступна только в платных версиях.";
				return result;
			}
			if (tSend < 1 || tSend > 4)
			{
				result.errCode = 91;
				result.errStr = "Указан ошибочный тип сообщения: " + tSend;
				return result;
			}
			string text = All.f.StringGetFn(All.A.FN, "PDF");
			if (Operators.CompareString(text, "1", false) == 0)
			{
				if (!GetCheckcloudurlForSMS(nTax))
				{
					text = "0";
					All.Lg.SaveTextToLog("SendPDF", "Помилка відправки PDF, лінк сформований на сайт податкової");
				}
			}
			else
			{
				text = "0";
			}
			TypPrintChecks typPrintChecks = All.Rf.CheckXMLNumberTax(nTax);
			string text2 = LongDate(typPrintChecks.ReturnStr);
			string sBablo = All.Rf.CheckSumNumberTax(nTax);
			if (Operators.CompareString(text2.Trim(), "", false) == 0)
			{
				result.errCode = 91;
				result.errStr = "Ошибка отправки! Чек " + nTax + " не найден.";
				return result;
			}
			TypErrStr typErrStr = TestNumPhone(nPhone);
			if (typErrStr.errCode > 0)
			{
				result.errCode = typErrStr.errCode;
				result.errStr = typErrStr.errStr;
				return result;
			}
			nPhone = typErrStr.ReturnStr;
			if (TypChekcs(typPrintChecks.ReturnStr) < 0)
			{
				result.errCode = 91;
				result.errStr = "Ошибка! Можно отправлять только товарный чек.";
				return result;
			}
			string text3 = "http://lic.webchek.com.ua/api.pl?";
			string text4 = "fn=" + All.A.FN;
			string text5 = "cid=" + nTax;
			string text6 = "dest=" + nPhone;
			string text7 = "ciddt=" + text2;
			string text8 = "sm=" + All.Bablo(sBablo);
			string text9 = "messagetype=" + tSend;
			string text10 = ((Operators.CompareString(text, "1", false) != 0) ? "pdf=0" : "pdf=1");
			string text11 = "";
			text11 += SS(nTax, tSend);
			text11 += SS(All.A.FN, tSend);
			text11 += SS(nPhone, tSend);
			string text12 = "hashcid=" + text11;
			text3 = text3 + text4 + "&";
			text3 = text3 + text5 + "&";
			text3 = text3 + text6 + "&";
			text3 = text3 + text7 + "&";
			text3 = text3 + text8 + "&";
			text3 = text3 + text10 + "&";
			text3 = text3 + text9 + "&";
			text3 += text12;
			All.Lg.SaveTextToLog("SendMessage", "Отправка чека на Vier", text5);
			string text13 = new WebClient().DownloadString(text3);
			if (Operators.CompareString(text13, (string)null, false) == 0)
			{
				result.errCode = 91;
				result.errStr = "Ошибка отправки в вайбер. Нет связи с сервером. Проверьте наличие интерента.";
			}
			else
			{
				text13 = text13.Trim();
				int num = ((!Versioned.IsNumeric((object)text13)) ? (-999) : Conversions.ToInteger(text13));
				int num2 = num;
				if (num2 > -1)
				{
					result.errCode = 0;
					result.errStr = text13;
				}
				else
				{
					switch (num2)
					{
					case -5:
						result.errCode = 91;
						result.errStr = "Баланс  сообщений 0";
						break;
					case -20:
						result.errCode = 91;
						result.errStr = "Повідомлення чекає на доставку абоненту";
						break;
					case -21:
						result.errCode = 91;
						result.errStr = "Помилка доставки повідомлення!";
						break;
					default:
					{
						string text14 = "Цей чек був відправлений ";
						if (Operators.CompareString(Conversions.ToString(text13[0]), "v", false) == 0)
						{
							text14 = text14 + Conversions.ToString(text13[9]) + Conversions.ToString(text13[10]) + "." + Conversions.ToString(text13[6]) + Conversions.ToString(text13[7]) + "." + Conversions.ToString(text13[1]) + Conversions.ToString(text13[2]) + Conversions.ToString(text13[3]) + Conversions.ToString(text13[4]);
							text14 = text14 + " " + Conversions.ToString(text13[12]) + Conversions.ToString(text13[13]) + ":" + Conversions.ToString(text13[15]) + Conversions.ToString(text13[16]);
							text14 += " VIBER";
							result.errCode = 91;
							result.errStr = text14;
						}
						else if (Operators.CompareString(Conversions.ToString(text13[0]), "s", false) == 0)
						{
							text14 = text14 + Conversions.ToString(text13[9]) + Conversions.ToString(text13[10]) + "." + Conversions.ToString(text13[6]) + Conversions.ToString(text13[7]) + "." + Conversions.ToString(text13[1]) + Conversions.ToString(text13[2]) + Conversions.ToString(text13[3]) + Conversions.ToString(text13[4]);
							text14 = text14 + " " + Conversions.ToString(text13[12]) + Conversions.ToString(text13[13]) + ":" + Conversions.ToString(text13[15]) + Conversions.ToString(text13[16]);
							text14 += " SMS";
							result.errCode = 91;
							result.errStr = text14;
						}
						else
						{
							result.errCode = 91;
							result.errStr = "Получен ответ от сервера: " + text13;
						}
						break;
					}
					}
				}
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 91;
			result.errStr = "Ошибка. Проверьте наличие интерента.";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal TypErr InTextViber()
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		try
		{
			string text = "http://lic.webchek.com.ua/api.pl?";
			string text2 = "fn=" + All.A.FN;
			string text3 = "";
			text3 += SS(All.A.FN, 4);
			string text4 = "hashcid=" + text3;
			text = text + text2 + "&";
			text += "messagetype=4&";
			text += text4;
			All.Lg.SaveTextToLog("SendMessage", "Запрос остатка отправок");
			string text5 = new WebClient().DownloadString(text);
			if (Operators.CompareString(text5, (string)null, false) == 0)
			{
				result.errCode = 92;
				result.errStr = "Ошибка получения числа отправок сообщений.";
			}
			else
			{
				text5 = text5.Trim();
				int num = ((!Versioned.IsNumeric((object)text5)) ? (-999) : Conversions.ToInteger(text5));
				if (num > -1)
				{
					result.errCode = 0;
					result.errStr = num.ToString();
				}
				else
				{
					result.errCode = 92;
					result.errStr = "Ошибка получения числа отправок в вайбер.";
				}
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result.errCode = 91;
			result.errStr = "Ошибка. Проверьте наличие интерента.";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	private TypErrStr TestNumPhone(string nPh)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		nPh = nPh.Trim();
		if (nPh.Length != 12)
		{
			result.errCode = 91;
			result.errStr = "Не верно указан номер телефона";
			return result;
		}
		if (Operators.CompareString(Conversions.ToString(nPh[0]), "3", false) != 0)
		{
			result.errCode = 91;
			result.errStr = "Не верно указан номер телефона";
			return result;
		}
		if (Operators.CompareString(Conversions.ToString(nPh[1]), "8", false) != 0)
		{
			result.errCode = 91;
			result.errStr = "Не верно указан номер телефона";
			return result;
		}
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = nPh;
		return result;
	}

	private string LongDate(string xmlC)
	{
		string result;
		try
		{
			XmlDocument xmlDocument = new XmlDocument();
			xmlDocument.LoadXml(xmlC);
			string innerText = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
			string text = LongToData(innerText, ForLink: true);
			string timeCheck = LongToTime(innerText);
			result = text + "&time=" + TimeToTimeWWW(timeCheck);
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

	private string LongToData(string LongDT, bool ForLink = false)
	{
		if (LongDT.Length != 14)
		{
			return "";
		}
		if (!ForLink)
		{
			return Conversions.ToString(LongDT[6]) + Conversions.ToString(LongDT[7]) + "." + Conversions.ToString(LongDT[4]) + Conversions.ToString(LongDT[5]) + "." + Conversions.ToString(LongDT[0]) + Conversions.ToString(LongDT[1]) + Conversions.ToString(LongDT[2]) + Conversions.ToString(LongDT[3]);
		}
		return Conversions.ToString(LongDT[0]) + Conversions.ToString(LongDT[1]) + Conversions.ToString(LongDT[2]) + Conversions.ToString(LongDT[3]) + Conversions.ToString(LongDT[4]) + Conversions.ToString(LongDT[5]) + Conversions.ToString(LongDT[6]) + Conversions.ToString(LongDT[7]);
	}

	private string LongToTime(string LongDT)
	{
		if (LongDT.Length != 14)
		{
			return "время";
		}
		return Conversions.ToString(LongDT[8]) + Conversions.ToString(LongDT[9]) + "-" + Conversions.ToString(LongDT[10]) + Conversions.ToString(LongDT[11]) + "-" + Conversions.ToString(LongDT[12]) + Conversions.ToString(LongDT[13]);
	}

	private string TimeToTimeWWW(string TimeCheck)
	{
		return Conversions.ToString(TimeCheck[0]) + Conversions.ToString(TimeCheck[1]) + Conversions.ToString(TimeCheck[3]) + Conversions.ToString(TimeCheck[4]);
	}

	private string SS(string strN, int intT)
	{
		strN = strN.Trim();
		if (Operators.CompareString(strN, "", false) == 0)
		{
			return "108";
		}
		checked
		{
			intT = 108 - intT;
			int num = strN.Length - 1;
			int num2 = 0;
			int num3 = num;
			for (int i = 0; i <= num3; i++)
			{
				string text = Conversions.ToString(strN[i]);
				num2 = ((!Versioned.IsNumeric((object)text)) ? (num2 + (9 + intT + i)) : (num2 + (Conversions.ToInteger(text) + intT + i)));
			}
			return num2.ToString();
		}
	}

	private int TypChekcs(string xmlCheck)
	{
		TypErrStr parametrToString = All.d.GetParametrToString(xmlCheck, "t", "rq/dat/c");
		if (parametrToString.errCode == 0)
		{
			if (Operators.CompareString(parametrToString.ReturnStr, "0", false) == 0)
			{
				return 0;
			}
			if (Operators.CompareString(parametrToString.ReturnStr, "1", false) == 0)
			{
				return 1;
			}
			return -1;
		}
		return -1;
	}

	private bool GetCheckcloudurlForSMS(string TaxNum)
	{
		TypErrStr typErrStr = default(TypErrStr);
		typErrStr.errCode = 0;
		typErrStr.errStr = "";
		typErrStr.ReturnStr = "";
		TypPrintChecks typPrintChecks = new Reports().CheckXMLNumberTax(TaxNum);
		if (typPrintChecks.ReturnStr.Trim().Length < 9)
		{
			All.Lg.SaveTextToLog("GetCheckcloudurlForSMS", TaxNum, "Ошибка! Чек в базе не найден.");
			return false;
		}
		if ((Operators.CompareString(typPrintChecks.ReturnDocType, "0", false) == 0) | (Operators.CompareString(typPrintChecks.ReturnDocType, "1", false) == 0))
		{
			typPrintChecks.ReturnDocType = typPrintChecks.ReturnDocType;
			string text = All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\" + typPrintChecks.ReturnStrTaxN + ".pdf";
			if (File.Exists(text))
			{
				try
				{
					FileSystem.DeleteFile(text);
				}
				catch (Exception ex)
				{
					ProjectData.SetProjectError(ex);
					Exception ex2 = ex;
					ProjectData.ClearProjectError();
				}
			}
			if (!All.AccessAWS())
			{
				All.Lg.SaveTextToLog("GetCheckcloudurlForSMS", TaxNum, "Ошибка! Нет доступа к серверу.");
				return false;
			}
			new PrintExportCheck().ExportCheckToPDF(text, typPrintChecks.ReturnStr, typPrintChecks.ReturnStrTaxN, lite: false);
			SendFilePDF sendFilePDF = new SendFilePDF();
			int num = 0;
			do
			{
				typErrStr = sendFilePDF.SendPDF(text, typPrintChecks.ReturnStrTaxN + ".pdf");
				if (typErrStr.errCode == 0)
				{
					break;
				}
				num = checked(num + 1);
			}
			while (num <= 1);
			if (typErrStr.errCode > 0)
			{
				All.Lg.SaveTextToLog("GetCheckcloudurlForSMS", TaxNum, typErrStr.errStr);
				return false;
			}
			return true;
		}
		All.Lg.SaveTextToLog("GetCheckcloudurlForSMS", TaxNum, "Ошибка! Тип чека не является публичным.");
		return false;
	}
}
