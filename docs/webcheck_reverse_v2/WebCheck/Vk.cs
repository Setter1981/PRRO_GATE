using System;
using System.Xml;
using Microsoft.VisualBasic.CompilerServices;

namespace WebCheck;

internal class Vk
{
	private XmlDocument x;

	public Vk()
	{
		x = new XmlDocument();
	}

	internal string XMLvkToCom(string xmlVK)
	{
		xmlVK = All.d.TegXml(xmlVK);
		string result;
		try
		{
			x.LoadXml(xmlVK);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = xmlVK;
			ProjectData.ClearProjectError();
			goto IL_0b3d;
		}
		string text;
		try
		{
			text = x.GetElementsByTagName("l")[0].OuterXml;
		}
		catch (Exception ex3)
		{
			ProjectData.SetProjectError(ex3);
			Exception ex4 = ex3;
			text = "";
			ProjectData.ClearProjectError();
		}
		string text2 = "";
		string text3 = "";
		string text4 = "";
		string text5 = "";
		string text6 = "";
		string value;
		try
		{
			value = x.SelectSingleNode("checkpackage/parameters/@paymenttype").Value;
		}
		catch (Exception ex5)
		{
			ProjectData.SetProjectError(ex5);
			Exception ex6 = ex5;
			result = xmlVK;
			ProjectData.ClearProjectError();
			goto IL_0b3d;
		}
		int num = Conversions.ToInteger(value);
		checked
		{
			num--;
			if (num < 0)
			{
				num = 0;
			}
			value = num.ToString();
			try
			{
				text6 = x.SelectSingleNode("checkpackage/parameters/@uuid").Value;
			}
			catch (Exception ex7)
			{
				ProjectData.SetProjectError(ex7);
				Exception ex8 = ex7;
				text6 = "";
				ProjectData.ClearProjectError();
			}
			string[] array = new string[All.PayTax.PayN + 1];
			bool flag = false;
			string text7;
			try
			{
				text7 = x.SelectSingleNode("checkpackage/payments/@smb").Value;
			}
			catch (Exception ex9)
			{
				ProjectData.SetProjectError(ex9);
				Exception ex10 = ex9;
				text7 = "";
				ProjectData.ClearProjectError();
			}
			int payN = All.PayTax.PayN;
			for (int i = 0; i <= payN; i++)
			{
				try
				{
					array[i] = x.SelectSingleNode("checkpackage/payments/@pay" + i).Value;
					if (All.StrToDouble(array[i]) > 0.0)
					{
						flag = true;
					}
				}
				catch (Exception ex11)
				{
					ProjectData.SetProjectError(ex11);
					Exception ex12 = ex11;
					array[i] = "0";
					ProjectData.ClearProjectError();
				}
			}
			if (!flag)
			{
				try
				{
					text2 = x.SelectSingleNode("checkpackage/payments/@cash").Value;
				}
				catch (Exception ex13)
				{
					ProjectData.SetProjectError(ex13);
					Exception ex14 = ex13;
					text2 = "0";
					ProjectData.ClearProjectError();
				}
				try
				{
					text3 = x.SelectSingleNode("checkpackage/payments/@electronicpayment").Value;
				}
				catch (Exception ex15)
				{
					ProjectData.SetProjectError(ex15);
					Exception ex16 = ex15;
					text3 = "0";
					ProjectData.ClearProjectError();
				}
				try
				{
					text4 = x.SelectSingleNode("checkpackage/payments/@credit").Value;
				}
				catch (Exception ex17)
				{
					ProjectData.SetProjectError(ex17);
					Exception ex18 = ex17;
					text4 = "0";
					ProjectData.ClearProjectError();
				}
				try
				{
					text5 = x.SelectSingleNode("checkpackage/payments/@advancepayment").Value;
				}
				catch (Exception ex19)
				{
					ProjectData.SetProjectError(ex19);
					Exception ex20 = ex19;
					text5 = "0";
					ProjectData.ClearProjectError();
				}
				try
				{
					_ = x.SelectSingleNode("checkpackage/payments/@cashprovision").Value;
				}
				catch (Exception ex21)
				{
					ProjectData.SetProjectError(ex21);
					Exception ex22 = ex21;
					ProjectData.ClearProjectError();
				}
			}
			XmlNodeList elementsByTagName = x.GetElementsByTagName("fiscalstring");
			int num2 = elementsByTagName.Count - 1;
			string[,] array2 = new string[12, num2 + 1];
			int num3 = num2;
			for (int j = 0; j <= num3; j++)
			{
				XmlDocument xmlDocument = new XmlDocument();
				xmlDocument.LoadXml(elementsByTagName[j].OuterXml);
				try
				{
					array2[0, j] = xmlDocument.SelectSingleNode("fiscalstring/@name").Value;
				}
				catch (Exception ex23)
				{
					ProjectData.SetProjectError(ex23);
					Exception ex24 = ex23;
					array2[0, j] = "нет";
					ProjectData.ClearProjectError();
				}
				try
				{
					array2[1, j] = xmlDocument.SelectSingleNode("fiscalstring/@quantity").Value;
				}
				catch (Exception ex25)
				{
					ProjectData.SetProjectError(ex25);
					Exception ex26 = ex25;
					array2[1, j] = "1";
					ProjectData.ClearProjectError();
				}
				try
				{
					array2[2, j] = xmlDocument.SelectSingleNode("fiscalstring/@pricewithdiscount").Value;
				}
				catch (Exception ex27)
				{
					ProjectData.SetProjectError(ex27);
					Exception ex28 = ex27;
					array2[2, j] = "0";
					ProjectData.ClearProjectError();
				}
				try
				{
					array2[3, j] = xmlDocument.SelectSingleNode("fiscalstring/@sumwithdiscount").Value;
				}
				catch (Exception ex29)
				{
					ProjectData.SetProjectError(ex29);
					Exception ex30 = ex29;
					array2[3, j] = "0";
					ProjectData.ClearProjectError();
				}
				try
				{
					array2[4, j] = xmlDocument.SelectSingleNode("fiscalstring/@discountsum").Value;
				}
				catch (Exception ex31)
				{
					ProjectData.SetProjectError(ex31);
					Exception ex32 = ex31;
					array2[4, j] = "0";
					ProjectData.ClearProjectError();
				}
				try
				{
					array2[5, j] = xmlDocument.SelectSingleNode("fiscalstring/@tax").Value;
				}
				catch (Exception ex33)
				{
					ProjectData.SetProjectError(ex33);
					Exception ex34 = ex33;
					array2[5, j] = "0";
					ProjectData.ClearProjectError();
				}
				try
				{
					array2[6, j] = xmlDocument.SelectSingleNode("fiscalstring/@price").Value;
				}
				catch (Exception ex35)
				{
					ProjectData.SetProjectError(ex35);
					Exception ex36 = ex35;
					array2[6, j] = "0";
					ProjectData.ClearProjectError();
				}
				try
				{
					array2[7, j] = xmlDocument.SelectSingleNode("fiscalstring/@uktzed").Value;
				}
				catch (Exception ex37)
				{
					ProjectData.SetProjectError(ex37);
					Exception ex38 = ex37;
					array2[7, j] = "";
					ProjectData.ClearProjectError();
				}
				try
				{
					array2[8, j] = xmlDocument.SelectSingleNode("fiscalstring/@excisestamp").Value;
				}
				catch (Exception ex39)
				{
					ProjectData.SetProjectError(ex39);
					Exception ex40 = ex39;
					array2[8, j] = "";
					ProjectData.ClearProjectError();
				}
				try
				{
					array2[9, j] = xmlDocument.SelectSingleNode("fiscalstring/@barcode").Value;
				}
				catch (Exception ex41)
				{
					ProjectData.SetProjectError(ex41);
					Exception ex42 = ex41;
					array2[9, j] = "";
					ProjectData.ClearProjectError();
				}
				try
				{
					array2[10, j] = xmlDocument.SelectSingleNode("fiscalstring/@avans").Value;
				}
				catch (Exception ex43)
				{
					ProjectData.SetProjectError(ex43);
					Exception ex44 = ex43;
					array2[10, j] = "";
					ProjectData.ClearProjectError();
				}
				try
				{
					array2[11, j] = xmlDocument.SelectSingleNode("fiscalstring/@avansm").Value;
				}
				catch (Exception ex45)
				{
					ProjectData.SetProjectError(ex45);
					Exception ex46 = ex45;
					array2[11, j] = "";
					ProjectData.ClearProjectError();
				}
				switch (array2[5, j])
				{
				case "none":
					array2[5, j] = "Ж";
					continue;
				case "20/5":
					array2[5, j] = "ГА";
					continue;
				case "0/5":
					array2[5, j] = "ГБ";
					continue;
				case "20/75":
					array2[5, j] = "ДА";
					continue;
				case "0/75":
					array2[5, j] = "ДБ";
					continue;
				}
				if (Versioned.IsNumeric(array2[5, j]))
				{
					array2[5, j] = All.PayTax.SearchTaxPr(array2[5, j]).TaxName;
				}
				else
				{
					array2[5, j] = array2[5, j].ToUpper();
				}
			}
			string text8 = ((Operators.CompareString(text6, "", TextCompare: false) != 0) ? ("<Check FN='" + All.A.FN + "' OperationType='" + value + "' uuid='" + text6 + "'>") : ("<Check FN='" + All.A.FN + "' OperationType='" + value + "'>"));
			text8 += "<Goods>";
			int num4 = num2;
			for (int j = 0; j <= num4; j++)
			{
				double num5 = All.StrToDouble(array2[6, j]);
				if (unchecked(num5 == 0.0 || num5 < 0.0))
				{
					num5 = All.StrToDouble(array2[3, j]) + All.StrToDouble(array2[4, j]);
					num5 /= All.StrToDouble(array2[1, j]);
				}
				text8 = text8 + "<Good Code ='000' Name='" + array2[0, j] + "' Quantity='" + array2[1, j] + "' Price='" + num5 + "' Sum='" + array2[3, j] + "' TaxRate='" + array2[5, j] + "' Uktzed='" + array2[7, j] + "' Excisestamp='" + array2[8, j];
				if (array2[9, j].Trim().Length > 0)
				{
					text8 = text8 + "' barcode='" + array2[9, j];
				}
				if (array2[10, j].Trim().Length > 0)
				{
					text8 = text8 + "' avans='" + array2[10, j];
				}
				if (array2[11, j].Trim().Length > 0)
				{
					text8 = text8 + "' avansm='" + array2[11, j];
				}
				text8 += "'/>";
			}
			text8 += "</Goods>";
			text8 += "<Payments>";
			if (!flag)
			{
				text8 = text8 + "<Payment ID = '1' Sum='" + text2 + "'/>";
				text8 = text8 + "<Payment ID = '2' Sum='" + text3 + "'/>";
				text8 = text8 + "<Payment ID = '3' Sum='" + text4 + "'/>";
				int num6 = All.l.SearchPayFormsID("Сертифікат");
				if (num6 > 0)
				{
					text8 = text8 + "<Payment ID = '" + num6 + "' Sum='" + text5 + "'/>";
				}
			}
			else
			{
				int payN2 = All.PayTax.PayN;
				for (int i = 0; i <= payN2; i++)
				{
					if (All.StrToDouble(array[i]) > 0.0)
					{
						string text9 = i.ToString();
						if (i == 0)
						{
							text9 = "1";
						}
						text8 = ((!((Operators.CompareString(text9, "1", TextCompare: false) == 0) & (Operators.CompareString(text7, "", TextCompare: false) != 0))) ? (text8 + "<Payment ID ='" + text9 + "' Sum='" + array[i] + "'/>") : (text8 + "<Payment ID ='" + text9 + "' Sum='" + array[i] + "' SMB='" + text7 + "'/>"));
					}
				}
			}
			text8 += "</Payments>";
			if (text.Trim().Length > 3)
			{
				text8 += text;
			}
			text8 += "</Check>";
			try
			{
				x.LoadXml(text8);
			}
			catch (Exception ex47)
			{
				ProjectData.SetProjectError(ex47);
				Exception ex48 = ex47;
				result = xmlVK;
				ProjectData.ClearProjectError();
				goto IL_0b3d;
			}
			result = text8;
			goto IL_0b3d;
		}
		IL_0b3d:
		return result;
	}
}
