using System;
using System.Xml;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

public class CheckStamp
{
	private int LinesInCheck;

	private string LincWWW;

	private string LCheckID;

	private string LMac;

	public int SizeCheck()
	{
		LinesInCheck = Secondary.CountLine;
		return LinesInCheck;
	}

	public string LineFromCheck(int index)
	{
		if (index > LinesInCheck)
		{
			return "";
		}
		if (index < 0)
		{
			return index switch
			{
				-1 => LMac, 
				-2 => LincWWW, 
				-3 => LCheckID, 
				_ => "", 
			};
		}
		return Secondary.GetCheckLine(index);
	}

	private bool CheckLoadToArray(string idTaxCheck)
	{
		bool result;
		if (!All.A.Status)
		{
			result = false;
		}
		else
		{
			try
			{
				TypPrintChecks typPrintChecks;
				if (Operators.CompareString(idTaxCheck.Trim(), "", TextCompare: false) == 0)
				{
					idTaxCheck = All.l.MaxID("ksef").ReturnStr;
					typPrintChecks = All.Rf.CheckXMLNumber(idTaxCheck.Trim(), SearchID: true);
				}
				else
				{
					typPrintChecks = All.Rf.CheckXMLNumberTax(idTaxCheck);
				}
				XmlDocument xmlDocument = new XmlDocument();
				xmlDocument.LoadXml(typPrintChecks.ReturnStr);
				string innerText = xmlDocument.GetElementsByTagName("ts")[0].InnerText;
				LincWWW = "https://cabinet.tax.gov.ua/cashregs/check?id=" + typPrintChecks.ReturnStrTaxN + "&amp;date=" + LongToData(innerText, ForLink: true) + "&amp;time=" + TimeToTimeWWW(LongToTime(innerText)) + "&amp;fn=" + All.A.FN + "&amp;sm=" + All.Bablo(typPrintChecks.ReturnSum) + "&amp;mac=" + typPrintChecks.ReturnMac;
				LCheckID = typPrintChecks.ReturnStrTaxN;
				LMac = typPrintChecks.ReturnMac;
				new PrintExportCheck().ExportToArray(typPrintChecks.ReturnStr, typPrintChecks.ReturnStrTaxN);
				result = true;
			}
			catch (Exception ex)
			{
				ProjectData.SetProjectError(ex);
				Exception ex2 = ex;
				LincWWW = "";
				LCheckID = "";
				LMac = "";
				result = false;
				ProjectData.ClearProjectError();
			}
		}
		return result;
	}

	private string LongToData(string LongDT, bool ForLink = false)
	{
		if (LongDT.Length != 14)
		{
			return "дата";
		}
		if (!ForLink)
		{
			return Conversions.ToString(LongDT[6]) + Conversions.ToString(LongDT[7]) + "." + Conversions.ToString(LongDT[4]) + Conversions.ToString(LongDT[5]) + "." + Conversions.ToString(LongDT[0]) + Conversions.ToString(LongDT[1]) + Conversions.ToString(LongDT[2]) + Conversions.ToString(LongDT[3]);
		}
		return Conversions.ToString(LongDT[0]) + Conversions.ToString(LongDT[1]) + Conversions.ToString(LongDT[2]) + Conversions.ToString(LongDT[3]) + Conversions.ToString(LongDT[4]) + Conversions.ToString(LongDT[5]) + Conversions.ToString(LongDT[6]) + Conversions.ToString(LongDT[7]);
	}

	private string TimeToTimeWWW(string TimeCheck)
	{
		return Conversions.ToString(TimeCheck[0]) + Conversions.ToString(TimeCheck[1]) + Conversions.ToString(TimeCheck[3]) + Conversions.ToString(TimeCheck[4]);
	}

	private string LongToTime(string LongDT)
	{
		if (LongDT.Length != 14)
		{
			return "время";
		}
		return Conversions.ToString(LongDT[8]) + Conversions.ToString(LongDT[9]) + "-" + Conversions.ToString(LongDT[10]) + Conversions.ToString(LongDT[11]) + "-" + Conversions.ToString(LongDT[12]) + Conversions.ToString(LongDT[13]);
	}

	public string CheckArrayToXML()
	{
		if (!All.A.Status)
		{
			return "";
		}
		if (SizeCheck() < 3)
		{
			return "";
		}
		string text = "<?xml version='1.0' encoding='utf8'?><OutputParameters><Parameters CL='" + SizeCheck() + "' WWW='" + LincWWW + "' CheckID='" + LCheckID + "' FN='" + All.A.FN + "' MAC='" + LMac + "' ";
		int num = SizeCheck();
		for (int i = 0; i <= num; i = checked(i + 1))
		{
			string expression = LineFromCheck(i);
			expression = Strings.Replace(expression, "&", "&amp;");
			expression = Strings.Replace(expression, "\"", "&quot;");
			expression = Strings.Replace(expression, "'", "&apos;");
			text = text + " L" + i + "='" + expression + "'";
		}
		return text + "/></OutputParameters>";
	}

	public string CheckXML(string idTaxCheck)
	{
		if (!All.A.Status)
		{
			return "";
		}
		LincWWW = "";
		LCheckID = "";
		LMac = "";
		if (CheckLoadToArray(idTaxCheck))
		{
			return CheckArrayToXML();
		}
		return "";
	}
}
